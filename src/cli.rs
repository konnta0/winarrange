use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Only arrange windows whose application/process name contains this value.
    #[arg(
        long,
        global = true,
        value_name = "NAME",
        conflicts_with = "focused_process"
    )]
    pub process: Option<String>,

    /// Arrange windows belonging to the currently focused window's process.
    #[arg(long, global = true, conflicts_with = "process")]
    pub focused_process: bool,

    /// Override the number of grid columns.
    #[arg(long, global = true, value_name = "COUNT")]
    pub columns: Option<usize>,

    /// Override the gap between windows in pixels.
    #[arg(long, global = true, value_name = "PIXELS")]
    pub gap: Option<i32>,

    /// Override the margin around each monitor work area in pixels.
    #[arg(long, global = true, value_name = "PIXELS")]
    pub margin: Option<i32>,

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
    /// Run a named profile from config.toml.
    Profile {
        /// Profile name.
        name: String,
    },
    /// List windows detected on the current Space / Virtual Desktop.
    List {
        /// Include the platform-specific window ID.
        #[arg(long)]
        verbose: bool,
    },
    /// Show configuration, profiles, floating windows, and ignore rules.
    Status,
}

#[derive(Clone, Debug, Subcommand)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v01_commands() {
        assert!(Cli::try_parse_from(["winarrange"]).is_ok());
        assert!(Cli::try_parse_from(["winarrange", "--process", "Unity"]).is_ok());
        assert!(Cli::try_parse_from(["winarrange", "toggle-float"]).is_ok());
        assert!(
            Cli::try_parse_from(["winarrange", "ignore", "add", "--process", "Calculator"]).is_ok()
        );
    }

    #[test]
    fn parses_v02_commands_and_overrides() {
        let focused = Cli::try_parse_from([
            "winarrange",
            "--focused-process",
            "--columns",
            "3",
            "--gap",
            "8",
            "--margin",
            "8",
            "--dry-run",
        ])
        .unwrap();
        assert!(focused.focused_process);
        assert_eq!(focused.columns, Some(3));

        let profile =
            Cli::try_parse_from(["winarrange", "profile", "unity", "--gap", "16", "--dry-run"])
                .unwrap();
        assert!(matches!(profile.command, Some(Command::Profile { name }) if name == "unity"));
        assert_eq!(profile.gap, Some(16));

        assert!(Cli::try_parse_from(["winarrange", "list", "--verbose"]).is_ok());
        assert!(Cli::try_parse_from(["winarrange", "status"]).is_ok());
    }

    #[test]
    fn process_and_focused_process_conflict() {
        assert!(
            Cli::try_parse_from(["winarrange", "--process", "Unity", "--focused-process"]).is_err()
        );
    }
}
