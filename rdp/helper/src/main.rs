//! `opensesh-rdp`: OpenSesh's RDP helper program (ADR 0034). The app starts it for an RDP pane and
//! talks to it over its standard input and output; it isn't meant to be run by hand.

use std::io::{BufReader, BufWriter};

use opensesh_rdp::session::Out;
use opensesh_rdp_protocol::{FromHelper, ToHelper};

fn main() -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("opensesh-rdp")
        .enable_all()
        .build()?;
    let (inbox_sender, inbox) = tokio::sync::mpsc::channel::<ToHelper>(1024);
    let (out_sender, mut outbox) = tokio::sync::mpsc::channel::<FromHelper>(64);
    // What the app says: read on a thread of its own (standard input blocks).
    std::thread::Builder::new()
        .name("opensesh-rdp-in".to_owned())
        .spawn(move || {
            let mut input = BufReader::new(std::io::stdin().lock());
            while let Ok(Some(message)) = ToHelper::read(&mut input) {
                if inbox_sender.blocking_send(message).is_err() {
                    break;
                }
            }
        })?;
    // What it hears: written on another, so a slow app slows the session (the channel is
    // bounded) without blocking the runtime.
    let writer = std::thread::Builder::new()
        .name("opensesh-rdp-out".to_owned())
        .spawn(move || {
            let mut output = BufWriter::new(std::io::stdout().lock());
            while let Some(message) = outbox.blocking_recv() {
                if message.write(&mut output).is_err() {
                    break;
                }
            }
        })?;
    runtime.block_on(opensesh_rdp::drive(inbox, Out(out_sender)));
    let _ = writer.join();
    Ok(())
}
