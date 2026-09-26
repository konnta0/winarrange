use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Only arrange windows whose application/process name contains this value.
    #[arg(long, global = true, value_name = "NAME")]
    pub process: Option<String>,

    /// Print the planned layout without moving any windows.
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Exclude the focused window from future arrange operations.
    Float,
    /// Include the focused window in future arrange operations.
    Unfloat,
    /// Toggle whether the focused window participates in arrangement.
    ToggleFloat,
    /// Manage persistent application ignore rules.
    Ignore {
        #[command(subcommand)]
        command: IgnoreCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum IgnoreCommand {
    /// Add an application/process ignore rule.
    Add {
        #[arg(long, value_name = "NAME")]
        process: String,
    },
    /// Remove an application/process ignore rule.
    Remove {
        #[arg(long, value_name = "NAME")]
        process: String,
    },
    /// List all persistent ignore rules.
    List,
}
