use anyhow::Result;
use clap::Parser;
use winarrange::{app, cli::Cli, platform};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let manager = platform::system_window_manager()?;
    app::run(cli, manager.as_ref())
}
