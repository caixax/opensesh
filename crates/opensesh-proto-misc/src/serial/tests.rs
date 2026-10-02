#![allow(clippy::unwrap_used, clippy::panic, reason = "tests")]

use super::*;

fn spec() -> SerialSpec {
    SerialSpec {
        device: "loop0".into(),
        baud: 115_200,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: 1,
        flow_control: FlowControl::None,
        newline: Newline::Cr,
        local_echo: false,
        log: None,
    }
}

#[test]
fn settings_in_short() {
    assert_eq!(spec().summary(), "115200 8N1");
    let other = SerialSpec {
        baud: 9600,
        data_bits: 7,
        parity: Parity::Even,
        stop_bits: 2,
        flow_control: FlowControl::Hardware,
        ..spec()
    };
    assert_eq!(other.summary(), "9600 7E2, RTS/CTS");
}

#[test]
fn enter_sends_what_the_device_wants() {
    assert_eq!(encode(b"show run\r", Newline::Cr), b"show run\r");
    assert_eq!(encode(b"ls\r", Newline::Lf), b"ls\n");
    assert_eq!(encode(b"ls\r", Newline::Crlf), b"ls\r\n");
}

#[test]
fn ports_sort_by_number() {
    let mut names = vec!["COM10", "COM3", "/dev/ttyUSB1", "COM1", "/dev/ttyUSB0"];
    names.sort_by_key(|name| natural(name));
    assert_eq!(
        names,
        ["/dev/ttyUSB0", "/dev/ttyUSB1", "COM1", "COM3", "COM10"]
    );
    // Listing works (whatever this computer has).
    let _ = ports();
}

#[test]
fn hex_lines() {
    let mut hex = HexView::default();
    let mut out = hex.format(b"Hello, ");
    assert!(hex.pending());
    out.extend(hex.format(b"world!\r\n\x00\x01\x02"));
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        text,
        "48 65 6C 6C 6F 2C 20 77  6F 72 6C 64 21 0D 0A 00  \u{1b}[2m|Hello, world!...|\u{1b}[0m\r\n01 02 "
    );
    let rest = String::from_utf8(hex.finish()).unwrap();
    // Padded to line the text column up with the full line's.
    assert_eq!(
        rest,
        format!("{}  \u{1b}[2m|..|\u{1b}[0m\r\n", " ".repeat(14 * 3))
    );
    assert!(!hex.pending() && hex.finish().is_empty());
}

fn read_until(events: &Receiver<BackendEvent>, wanted: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut text = String::new();
    while !text.contains(wanted) {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(BackendEvent::Output(bytes)) => text.push_str(&String::from_utf8_lossy(&bytes)),
            Ok(other) => panic!("{other:?} before {wanted:?} in {text:?}"),
            Err(_) => panic!("no {wanted:?} in {text:?}"),
        }
    }
    text
}

#[test]
fn a_session_on_the_loopback() {
    let spec = SerialSpec {
        newline: Newline::Crlf,
        local_echo: true,
        ..spec()
    };
    let (backend, events, control) = start_with(spec, || Ok(testing::loopback())).unwrap();
    let text = read_until(&events, "test device");
    assert!(text.contains("Opening loop0 (115200 8N1)"), "{text:?}");
    // Local echo, then the loopback's echo of what was sent (CR LF for Enter).
    backend.write(b"AT\r").unwrap();
    let text = read_until(&events, "AT\r\nAT\r\n");
    assert!(text.ends_with("AT\r\nAT\r\n"), "{text:?}");
    // In hex.
    control.set_hex(true);
    backend.write(b"Hi").unwrap();
    let text = read_until(&events, "|Hi|");
    assert!(
        text.contains("(hexadecimal view)") && text.contains("48 69 "),
        "{text:?}"
    );
    control.send_break();
    read_until(&events, "Break sent.");
    backend.shutdown();
}

#[test]
fn a_port_that_isnt_there_says_why() {
    let (_backend, events, _control) = start(SerialSpec {
        device: if cfg!(windows) {
            "COM250".into()
        } else {
            "/dev/opensesh-no-such-port".into()
        },
        ..spec()
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(BackendEvent::Error(reason)) => {
                assert!(
                    reason.contains("COM250") || reason.contains("opensesh-no-such-port"),
                    "{reason}"
                );
                break;
            }
            Ok(_) => {}
            Err(_) => panic!("no error"),
        }
    }
}
