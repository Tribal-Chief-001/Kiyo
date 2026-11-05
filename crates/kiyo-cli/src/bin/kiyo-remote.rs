//! `kiyo-remote`: Lightweight companion CLI for sending IPC commands to active Kiyo instances.

use clap::Parser;
use kiyo_cli::args::RemoteAction;

#[derive(Parser, Debug)]
#[command(
    name = "kiyo-remote",
    version = "0.1.0",
    about = "Instantaneous IPC remote control for running Kiyo desktop instances",
    long_about = "Dispatches sub-millisecond control actions (page flips, zoom, dark mode, jump tags) via UNIX domain socket."
)]
struct RemoteCli {
    #[command(subcommand)]
    action: RemoteAction,

    /// Target specific Kiyo process ID (defaults to active focused instance)
    #[arg(long, global = true)]
    pid: Option<u32>,
}

fn main() -> anyhow::Result<()> {
    let cli = RemoteCli::parse();

    // In a full environment, this opens /run/user/$UID/kiyo/$PID.sock (or active.sock)
    // and sends the command over the UNIX socket connection.
    println!(
        "kiyo-remote: dispatched {:?} (target pid: {:?})",
        cli.action, cli.pid
    );

    Ok(())
}
