use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "thurbox-orchestrator",
    version,
    about = "thurbox orchestrator CLI (scaffold — not ready for use)"
)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("thurbox-orchestrator {}", env!("CARGO_PKG_VERSION"));
}
