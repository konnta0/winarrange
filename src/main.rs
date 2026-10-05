use anyhow::Result;
use clap::Parser;
use winarrange::{
    app,
    cli::{Cli, Command},
    daemon, platform,
};

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Daemon { command }) = &cli.command {
        return daemon::command(command.clone());
    }
    let manager = platform::system_window_manager()?;
    app::run(cli, manager.as_ref())
}
