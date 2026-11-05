use clap::Parser;
use kiyo_cli::args::Cli;

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    println!("Kiyo PDF Engine v0.1.0");
    Ok(())
}
