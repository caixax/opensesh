#![allow(clippy::unwrap_used, clippy::panic, reason = "tests")]

use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;

fn size(columns: u16, lines: u16) -> TermSize {
    TermSize::new(columns, lines)
}

#[test]
fn the_server_negotiates_and_each_option_is_answered_once() {
    let mut nvt = Nvt::new("xterm-256color", size(80, 24));
    let offers = [
        IAC,
        WILL,
        option::ECHO,
        IAC,
        WILL,
        option::SGA,
        IAC,
        DO,
        option::TTYPE,
        IAC,
        DO,
        option::NAWS,
        // LINEMODE and STATUS aren't wanted.
        IAC,
        DO,
        34,
        IAC,
        WILL,
        5,
    ];
    let received = nvt.receive(&offers);
    assert!(received.data.is_empty());
    assert_eq!(
        received.reply,
        [
            IAC,
            DO,
            option::ECHO,
            IAC,
            DO,
            option::SGA,
            IAC,
            WILL,
            option::TTYPE,
            IAC,
            WILL,
            option::NAWS,
            IAC,
            SB,
            option::NAWS,
            0,
            80,
            0,
            24,
            IAC,
            SE,
            IAC,
            WONT,
            34,
            IAC,
            DONT,
            5,
        ]
    );
    assert!(nvt.server_echoes() && nvt.ours(option::NAWS) && nvt.ours(option::TTYPE));
    // The same offers again change nothing: no answer (no loop).
    assert!(
        nvt.receive(&[IAC, WILL, option::ECHO, IAC, DO, option::NAWS])
            .reply
            .is_empty()
    );
    // The server stops echoing: acknowledged once.
    assert_eq!(
        nvt.receive(&[IAC, WONT, option::ECHO]).reply,
        [IAC, DONT, option::ECHO]
    );
    assert!(!nvt.server_echoes());
    assert!(nvt.receive(&[IAC, WONT, option::ECHO]).reply.is_empty());
    assert_eq!(
        nvt.receive(&[IAC, DONT, option::NAWS]).reply,
        [IAC, WONT, option::NAWS]
    );
}

#[test]
fn the_terminal_type_is_sent_when_asked() {
    let mut nvt = Nvt::new("xterm-256color", size(80, 24));
    // Not before the client agreed to the option.
    assert!(
        nvt.receive(&[IAC, SB, option::TTYPE, TTYPE_SEND, IAC, SE])
            .reply
            .is_empty()
    );
    nvt.receive(&[IAC, DO, option::TTYPE]);
    let mut expected = vec![IAC, SB, option::TTYPE, TTYPE_IS];
    expected.extend(b"XTERM-256COLOR");
    expected.extend([IAC, SE]);
    assert_eq!(
        nvt.receive(&[IAC, SB, option::TTYPE, TTYPE_SEND, IAC, SE])
            .reply,
        expected
    );
}

#[test]
fn text_comes_through_in_any_pieces() {
    let mut nvt = Nvt::new("vt100", size(80, 24));
    let stream: Vec<u8> = [
        b"login:\r\0 a\r\nb".as_slice(),
        &[IAC, IAC, b'c'],
        &[IAC, 241],
        b"d",
    ]
    .concat();
    let whole = nvt.clone().receive(&stream);
    assert_eq!(whole.data, b"login:\r a\r\nb\xffcd");
    assert!(whole.reply.is_empty());
    // Byte by byte, the same.
    let mut data = Vec::new();
    for byte in &stream {
        data.extend(nvt.receive(std::slice::from_ref(byte)).data);
    }
    assert_eq!(data, whole.data);
    // A negotiation split between reads.
    let mut nvt = Nvt::new("vt100", size(80, 24));
    assert!(nvt.receive(&[b'x', IAC]).reply.is_empty());
    assert_eq!(
        nvt.receive(&[WILL, option::ECHO, b'y']),
        Received {
            data: b"y".to_vec(),
            reply: vec![IAC, DO, option::ECHO]
        }
    );
}

#[test]
fn window_sizes_are_sent_once_asked_and_escape_255() {
    let mut nvt = Nvt::new("vt100", size(80, 24));
    assert!(nvt.resize(size(100, 30)).is_empty());
    let reply = nvt.receive(&[IAC, DO, option::NAWS]).reply;
    assert_eq!(
        reply,
        [
            IAC,
            WILL,
            option::NAWS,
            IAC,
            SB,
            option::NAWS,
            0,
            100,
            0,
            30,
            IAC,
            SE
        ]
    );
    assert_eq!(
        nvt.resize(size(255, 511)),
        [IAC, SB, option::NAWS, 0, 255, 255, 1, 255, 255, IAC, SE]
    );
}

#[test]
fn what_is_typed_is_encoded() {
    let mut nvt = Nvt::new("vt100", size(80, 24));
    assert_eq!(nvt.encode(b"ls\r"), b"ls\r\n");
    // A LF right after the CR is already the end of line, also across calls.
    assert_eq!(nvt.encode(b"a\r"), b"a\r\n");
    assert_eq!(nvt.encode(b"\nb"), b"b");
    assert_eq!(nvt.encode(&[0xff, b'x']), [0xff, 0xff, b'x']);
    // In binary mode a CR is just a byte.
    nvt.receive(&[IAC, DO, option::BINARY]);
    assert_eq!(nvt.encode(b"\r"), b"\r");
    assert_eq!(local_echo(b"ab\x7fc\r\x1b[A"), b"ab\x08 \x08c\r\n[A");
}

/// A telnet server on the loopback for one client: it offers echo and asks for the window size,
/// prints a prompt, and hands back what it got.
async fn server() -> (u16, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket
            .write_all(&[IAC, WILL, option::ECHO, IAC, DO, option::NAWS])
            .await
            .unwrap();
        socket.write_all(b"login: ").await.unwrap();
        let mut got = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !got.ends_with(b"root\r\n") {
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(count > 0, "the client closed early: {got:?}");
            got.extend(&buffer[..count]);
        }
        socket.write_all(b"root\r\n# ").await.unwrap();
        got
    });
    (port, task)
}

fn read_until(events: &crossbeam_channel::Receiver<BackendEvent>, wanted: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut text = String::new();
    while !text.contains(wanted) {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(BackendEvent::Output(bytes)) => text.push_str(&String::from_utf8_lossy(&bytes)),
            Ok(other) => panic!("{other:?} before {wanted:?} in {text:?}"),
            Err(_) => panic!("no {wanted:?} in {text:?}"),
        }
    }
    text
}

#[test]
fn a_session_with_a_loopback_server() {
    let runtime = opensesh_ssh::runtime().unwrap();
    let (port, task) = runtime.block_on(server());
    let spec = TelnetSpec {
        host: "127.0.0.1".into(),
        port,
        term: "xterm-256color".into(),
        connect_timeout: Duration::from_secs(5),
        log: None,
    };
    let (backend, events) = start(spec, size(90, 30)).unwrap();
    let text = read_until(&events, "login: ");
    assert!(
        text.contains("Telnet sends everything in clear"),
        "{text:?}"
    );
    assert!(
        text.contains(&format!("Connecting to 127.0.0.1:{port}")),
        "{text:?}"
    );
    backend.write(b"root\r").unwrap();
    read_until(&events, "# ");
    let got = runtime.block_on(task).unwrap();
    // The answers to the offers and the window size, then what was typed (the server echoes, so
    // no local echo appeared before its own).
    assert_eq!(
        got,
        [
            &[
                IAC,
                DO,
                option::ECHO,
                IAC,
                WILL,
                option::NAWS,
                IAC,
                SB,
                option::NAWS,
                0,
                90,
                0,
                30,
                IAC,
                SE
            ][..],
            b"root\r\n"
        ]
        .concat()
    );
    // The server is gone: the session ends.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(BackendEvent::Exited(None)) => break,
            Ok(_) => {}
            Err(_) => panic!("the session didn't end"),
        }
    }
    backend.shutdown();
}

#[test]
fn nobody_listening_says_why() {
    let runtime = opensesh_ssh::runtime().unwrap();
    // A port that was free a moment ago.
    let port = runtime.block_on(async {
        TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    });
    let spec = TelnetSpec {
        host: "127.0.0.1".into(),
        port,
        term: "vt100".into(),
        connect_timeout: Duration::from_secs(5),
        log: None,
    };
    let (_backend, events) = start(spec, size(80, 24)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(BackendEvent::Error(reason)) => {
                assert!(reason.contains("could not connect"), "{reason}");
                break;
            }
            Ok(_) => {}
            Err(_) => panic!("no error"),
        }
    }
}
