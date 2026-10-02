use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Limit arrangement or focus selection to a matching application/process.
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
    /// Focus a visible window and activate its application.
    Focus(FocusArgs),
    /// Manage persistent application ignore rules.
    Ignore {
        #[command(subcommand)]
        command: IgnoreCommand,
    },
    /// Run or inspect named profiles from config.toml.
    Profile {
        /// Profile name (backward-compatible shorthand for running it).
        #[arg(value_name = "NAME")]
        name: Option<String>,
        #[command(subcommand)]
        command: Option<ProfileCommand>,
    },
    /// Inspect the configuration location.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// List windows detected on the current Space / Virtual Desktop.
    List {
        /// Include the platform-specific window ID.
        #[arg(long, conflicts_with = "json")]
        verbose: bool,
        /// Print machine-readable JSON including PID and window ID.
        #[arg(long, conflicts_with = "verbose")]
        json: bool,
    },
    /// Show configuration, profiles, floating windows, and ignore rules.
    Status,
}

#[derive(Clone, Debug, Args)]
#[command(group(
    ArgGroup::new("focus_target")
        .required(true)
        .multiple(false)
        .args(["pid", "title", "next", "prev"])
))]
pub struct FocusArgs {
    /// Focus a window belonging to this exact process ID.
    #[arg(long, value_name = "PID")]
    pub pid: Option<u32>,
    /// Focus the first stable, case-insensitive partial title match.
    #[arg(long, value_name = "TEXT")]
    pub title: Option<String>,
    /// Focus the next window in stable order, wrapping at the end.
    #[arg(long)]
    pub next: bool,
    /// Focus the previous window in stable order, wrapping at the beginning.
    #[arg(long)]
    pub prev: bool,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ProfileCommand {
    /// List configured profile names.
    List,
    /// Show one profile's effective configuration.
    Show { name: String },
}

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the config.toml path.
    Path,
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
        assert!(
            matches!(profile.command, Some(Command::Profile { name: Some(name), command: None }) if name == "unity")
        );
        assert_eq!(profile.gap, Some(16));

        assert!(Cli::try_parse_from(["winarrange", "list", "--verbose"]).is_ok());
        assert!(Cli::try_parse_from(["winarrange", "list", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["winarrange", "focus", "--pid", "12345"]).is_ok());
        assert!(Cli::try_parse_from(["winarrange", "focus", "--title", "Client"]).is_ok());
        assert!(
            Cli::try_parse_from(["winarrange", "focus", "--next", "--process", "Unity"]).is_ok()
        );
        assert!(Cli::try_parse_from(["winarrange", "focus", "--prev"]).is_ok());
        let profile_list = Cli::try_parse_from(["winarrange", "profile", "list"]).unwrap();
        assert!(matches!(
            profile_list.command,
            Some(Command::Profile {
                name: None,
                command: Some(ProfileCommand::List)
            })
        ));
        let profile_show = Cli::try_parse_from(["winarrange", "profile", "show", "unity"]).unwrap();
        assert!(matches!(
            profile_show.command,
            Some(Command::Profile {
                name: None,
                command: Some(ProfileCommand::Show { name })
            }) if name == "unity"
        ));
        assert!(Cli::try_parse_from(["winarrange", "config", "path"]).is_ok());
        assert!(Cli::try_parse_from(["winarrange", "status"]).is_ok());
    }

    #[test]
    fn focus_requires_exactly_one_selector() {
        assert!(Cli::try_parse_from(["winarrange", "focus"]).is_err());
        assert!(Cli::try_parse_from(["winarrange", "focus", "--next", "--prev"]).is_err());
    }

    #[test]
    fn process_and_focused_process_conflict() {
        assert!(
            Cli::try_parse_from(["winarrange", "--process", "Unity", "--focused-process"]).is_err()
        );
    }
}
