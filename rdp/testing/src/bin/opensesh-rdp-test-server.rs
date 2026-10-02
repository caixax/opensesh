//! The RDP test server as a program, for the app's smoke test (never shipped): it prints
//! `port=<port>` and serves until its standard input closes.

use std::io::{Read as _, Write as _};

fn main() -> std::io::Result<()> {
    let server = opensesh_rdp_testing::serve()?;
    // The port, for whoever started it (a line on standard output).
    let mut out = std::io::stdout().lock();
    writeln!(out, "port={}", server.port)?;
    out.flush()?;
    drop(out);
    // Until whoever started it closes our input (or ends).
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
    Ok(())
}
