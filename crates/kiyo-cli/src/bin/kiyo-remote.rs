//! `kiyo-remote`: Lightweight companion CLI for sending IPC commands to active Kiyo instances.

use clap::Parser;
use kiyo_cli::args::RemoteAction;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

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

    let dir = kiyo_gui::IpcServer::socket_dir();
    let socket_path = if let Some(pid) = cli.pid {
        dir.join(format!("{pid}.sock"))
    } else {
        dir.join("active.sock")
    };

    if !socket_path.exists() {
        println!("No active Kiyo instance found at {:?}", socket_path);
        return Ok(());
    }

    let mut stream = UnixStream::connect(&socket_path)?;
    let json_bytes = serde_json::to_vec(&cli.action)?;
    stream.write_all(&json_bytes)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if !response.trim().is_empty() {
        println!("{response}");
    } else {
        println!("kiyo-remote: action dispatched successfully.");
    }

    Ok(())
}
