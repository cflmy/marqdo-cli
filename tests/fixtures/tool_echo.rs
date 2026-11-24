//! Portable external-tool fixture used by `crates/cli/tests/tool_protocol.rs`.
//!
//! It intentionally exercises only std: argument forwarding, protocol
//! environment variables and a non-zero exit code. Compile it with rustc; do
//! not add it to the workspace (this keeps the fixture usable on Windows and
//! macOS without shell assumptions).

use std::io::Read;

fn main() {
    for argument in std::env::args().skip(1) {
        println!("arg={argument}");
    }
    println!(
        "protocol={}",
        std::env::var("MARQDO_API_REVISION").unwrap_or_default()
    );
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    if !stdin.is_empty() {
        print!("stdin={stdin}");
    }
    std::process::exit(7);
}
