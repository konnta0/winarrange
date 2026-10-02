use std::collections::{BTreeMap, HashSet};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::{
    cli::{Cli, Command, ConfigCommand, FocusArgs, IgnoreCommand, ProfileCommand},
    config::{
        active_config_path, config_path, Config, IgnoreRule, LayoutKind, Profile, SlotRule,
        SortOrder,
    },
    layout::{self, GridOptions},
    model::{Monitor, Rect, Window},
    platform::WindowManager,
    state::State,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Selection {
    All,
    ProcessContains(String),
    ProcessExact(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EffectiveSettings {
    layout: LayoutKind,
    sort: SortOrder,
    columns: Option<usize>,
    gap: i32,
    margin: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct CliOverrides {
    columns: Option<usize>,
    gap: Option<i32>,
    margin: Option<i32>,
}

pub fn run(cli: Cli, manager: &dyn WindowManager) -> Result<()> {
    let overrides = CliOverrides {
        columns: cli.columns,
        gap: cli.gap,
        margin: cli.margin,
    };
    match cli.command {
        None => arrange_request(
            manager,
            cli.process,
            cli.focused_process,
            None,
            overrides,
            cli.dry_run,
        ),
        Some(Command::Profile { name, command }) => match (name, command) {
            (Some(name), None) => arrange_request(
                manager,
                cli.process,
                cli.focused_process,
                Some(name),
                overrides,
                cli.dry_run,
            ),
            (None, Some(command)) => inspect_profile(command, overrides),
            _ => bail!("a profile name or inspection command is required"),
        },
        Some(Command::Float) => change_float(manager, FloatAction::Add, cli.dry_run),
        Some(Command::Unfloat) => change_float(manager, FloatAction::Remove, cli.dry_run),
        Some(Command::ToggleFloat) => change_float(manager, FloatAction::Toggle, cli.dry_run),
        Some(Command::Focus(args)) => focus_window(manager, args, cli.process, cli.dry_run),
        Some(Command::Config {
            command: ConfigCommand::Path,
        }) => {
            println!("{}", config_path()?.display());
            Ok(())
        }
        Some(Command::Ignore { command }) => change_ignore(command, cli.dry_run),
        Some(Command::List { verbose, json }) => list_windows(manager, verbose, json),
        Some(Command::Status) => show_status(manager),
    }
}

fn arrange_request(
    manager: &dyn WindowManager,
    cli_process: Option<String>,
    focused_process: bool,
    explicit_profile: Option<String>,
    overrides: CliOverrides,
    dry_run: bool,
) -> Result<()> {
    let config = Config::load()?;
    let profile_name = explicit_profile.or_else(|| config.default_profile.clone());
    let profile = profile_name
        .as_deref()
        .map(|name| {
            config
                .profiles
                .get(name)
                .with_context(|| format!("Profile '{name}' not found."))
        })
        .transpose()?;
    let settings = resolve_settings(&config, profile, overrides)?;

    let selection = if focused_process {
        let Some(focused) = manager.focused_window()? else {
            println!("No focused window found.");
            return Ok(());
        };
        Selection::ProcessExact(focused.process)
    } else if let Some(process) = cli_process {
        Selection::ProcessContains(process)
    } else if let Some(profile) = profile {
        if profile.process.trim().is_empty() {
            bail!("profile process must not be empty");
        }
        Selection::ProcessContains(profile.process.clone())
    } else {
        Selection::All
    };

    let slots = profile
        .map(|profile| profile.slots.as_slice())
        .unwrap_or(&[]);
    arrange_windows(manager, &config, selection, settings, slots, dry_run)
}

fn resolve_settings(
    config: &Config,
    profile: Option<&Profile>,
    cli: CliOverrides,
) -> Result<EffectiveSettings> {
    let settings = EffectiveSettings {
        layout: profile
            .and_then(|p| p.layout)
            .or(config.layout)
            .unwrap_or_default(),
        sort: profile
            .and_then(|p| p.sort)
            .or(config.sort)
            .unwrap_or_default(),
        columns: cli
            .columns
            .or_else(|| profile.and_then(|p| p.columns))
            .or(config.columns),
        gap: cli
            .gap
            .or_else(|| profile.and_then(|p| p.gap))
            .or(config.gap)
            .unwrap_or(0),
        margin: cli
            .margin
            .or_else(|| profile.and_then(|p| p.margin))
            .or(config.margin)
            .unwrap_or(0),
    };
    if settings.columns == Some(0) {
        bail!("columns must be greater than zero");
    }
    if settings.gap < 0 {
        bail!("gap must not be negative");
    }
    if settings.margin < 0 {
        bail!("margin must not be negative");
    }
    Ok(settings)
}

fn arrange_windows(
    manager: &dyn WindowManager,
    config: &Config,
    selection: Selection,
    settings: EffectiveSettings,
    slots: &[SlotRule],
    dry_run: bool,
) -> Result<()> {
    let state = State::load()?;
    let all_windows = manager.visible_windows()?;
    let mut windows = select_windows(all_windows, &selection, config, &state);
    sort_windows(&mut windows, settings.sort);

    if windows.is_empty() {
        println!("No matching windows found.");
        return Ok(());
    }

    let monitors = manager.monitors()?;
    let plans = build_plan(&windows, &monitors, settings, slots)?;
    for plan in plans {
        if dry_run {
            print_plan(plan.window, plan.monitor, plan.slot, plan.bounds);
        } else {
            manager
                .set_bounds(plan.window, plan.bounds)
                .with_context(|| {
                    format!(
                        "failed to move {} - {}",
                        plan.window.process, plan.window.title
                    )
                })?;
        }
    }
    Ok(())
}

fn select_windows(
    windows: Vec<Window>,
    selection: &Selection,
    config: &Config,
    state: &State,
) -> Vec<Window> {
    windows
        .into_iter()
        .filter(|window| match selection {
            Selection::All => true,
            Selection::ProcessContains(needle) => {
                contains_case_insensitive(&window.process, needle)
            }
            Selection::ProcessExact(process) => window.process.eq_ignore_ascii_case(process),
        })
        .filter(|window| !config.ignores_process(&window.process))
        .filter(|window| !state.contains(&window.key))
        .collect()
}

fn sort_windows(windows: &mut [Window], sort: SortOrder) {
    match sort {
        SortOrder::Title => windows.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.process.to_lowercase().cmp(&b.process.to_lowercase()))
                .then_with(|| a.key.pid.cmp(&b.key.pid))
                .then_with(|| a.key.id.cmp(&b.key.id))
        }),
    }
}

struct Plan<'a> {
    window: &'a Window,
    monitor: &'a Monitor,
    slot: usize,
    bounds: Rect,
}

fn build_plan<'a>(
    windows: &'a [Window],
    monitors: &'a [Monitor],
    settings: EffectiveSettings,
    slot_rules: &[SlotRule],
) -> Result<Vec<Plan<'a>>> {
    let monitor_map: BTreeMap<_, _> = monitors.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut groups: BTreeMap<&str, Vec<&Window>> = BTreeMap::new();
    for window in windows {
        if monitor_map.contains_key(window.monitor_id.as_str()) {
            groups.entry(&window.monitor_id).or_default().push(window);
        }
    }

    let mut plans = Vec::with_capacity(windows.len());
    for (monitor_id, group) in groups {
        let monitor = monitor_map[monitor_id];
        let assigned = assign_slots(group, slot_rules);
        let cell_count = assigned.iter().map(|(slot, _)| slot + 1).max().unwrap_or(0);
        let bounds = match settings.layout {
            LayoutKind::Grid => layout::arrange_with_options(
                cell_count,
                monitor.work_area,
                GridOptions {
                    columns: settings.columns,
                    gap: settings.gap,
                    margin: settings.margin,
                },
            )?,
        };
        plans.extend(assigned.into_iter().map(|(slot, window)| Plan {
            window,
            monitor,
            slot,
            bounds: bounds[slot],
        }));
    }
    Ok(plans)
}

fn assign_slots<'a>(windows: Vec<&'a Window>, rules: &[SlotRule]) -> Vec<(usize, &'a Window)> {
    let mut assigned_keys = HashSet::new();
    let mut occupied = BTreeMap::new();

    // Rules are evaluated in config order. A rule claims the first still-unassigned
    // window in the shared stable ordering; subsequent matches fall back normally.
    for rule in rules {
        if let Some(window) = windows.iter().copied().find(|window| {
            !assigned_keys.contains(&window.key)
                && contains_case_insensitive(&window.title, &rule.title)
        }) {
            let slot = usize::try_from(rule.position)
                .expect("validated slot positions are non-negative and fit usize");
            occupied.insert(slot, window);
            assigned_keys.insert(window.key.clone());
        }
    }

    let mut next_slot = 0;
    for window in windows {
        if assigned_keys.contains(&window.key) {
            continue;
        }
        while occupied.contains_key(&next_slot) {
            next_slot += 1;
        }
        occupied.insert(next_slot, window);
        next_slot += 1;
    }
    occupied.into_iter().collect()
}

fn print_plan(window: &Window, monitor: &Monitor, slot: usize, bounds: Rect) {
    println!(
        "{} - {}\n  monitor: {}\n  slot: {}\n  x: {}\n  y: {}\n  width: {}\n  height: {}\n",
        window.process,
        window.title,
        monitor.id,
        slot,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height
    );
}

enum FloatAction {
    Add,
    Remove,
    Toggle,
}

fn change_float(manager: &dyn WindowManager, action: FloatAction, dry_run: bool) -> Result<()> {
    let focused = manager
        .focused_window()?
        .context("No focused window found.")?;
    let mut state = State::load()?;
    let now_floating = match action {
        FloatAction::Add => {
            state.insert(focused.key);
            true
        }
        FloatAction::Remove => {
            state.remove(&focused.key);
            false
        }
        FloatAction::Toggle if state.contains(&focused.key) => {
            state.remove(&focused.key);
            false
        }
        FloatAction::Toggle => {
            state.insert(focused.key);
            true
        }
    };
    if !dry_run {
        state.save()?;
    }
    println!(
        "{} - {}: {}{}",
        focused.process,
        focused.title,
        if now_floating { "floating" } else { "managed" },
        if dry_run { " (dry run)" } else { "" }
    );
    Ok(())
}

fn change_ignore(command: IgnoreCommand, dry_run: bool) -> Result<()> {
    let mut config = Config::load()?;
    match command {
        IgnoreCommand::Add { process } => {
            if process.trim().is_empty() {
                bail!("process name cannot be empty");
            }
            if config.add_process(process.clone()) {
                if !dry_run {
                    config.save()?;
                }
                println!(
                    "{} process: {process}",
                    if dry_run { "Would ignore" } else { "Ignored" }
                );
            } else {
                println!("Process is already ignored: {process}");
            }
        }
        IgnoreCommand::Remove { process } => {
            if config.remove_process(&process) {
                if !dry_run {
                    config.save()?;
                }
                println!(
                    "{} ignore rule: {process}",
                    if dry_run { "Would remove" } else { "Removed" }
                );
            } else {
                bail!("ignore rule not found: {process}");
            }
        }
        IgnoreCommand::List => {
            for rule in config.ignore {
                match rule {
                    IgnoreRule::Process { value } => println!("process: {value}"),
                }
            }
        }
    }
    Ok(())
}

fn focus_window(
    manager: &dyn WindowManager,
    args: FocusArgs,
    process: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let mut windows = manager.visible_windows()?;
    sort_windows(&mut windows, SortOrder::Title);

    let selected = if let Some(pid) = args.pid {
        windows
            .iter()
            .find(|window| window.key.pid == pid)
            .with_context(|| format!("No visible window found for pid {pid}."))?
    } else if let Some(title) = args.title {
        if title.trim().is_empty() {
            bail!("focus title must not be empty");
        }
        let matches: Vec<_> = windows
            .iter()
            .filter(|window| {
                contains_case_insensitive(&window.title, &title)
                    && process
                        .as_ref()
                        .is_none_or(|process| contains_case_insensitive(&window.process, process))
            })
            .collect();
        let selected = matches
            .first()
            .copied()
            .with_context(|| format!("No visible window found matching title '{title}'."))?;
        if matches.len() > 1 {
            eprintln!(
                "{} windows matched; selecting the first in stable order: {} - {} (pid {}, window {})",
                matches.len(),
                selected.process,
                selected.title,
                selected.key.pid,
                selected.key.id
            );
        }
        selected
    } else {
        let direction = if args.next {
            Direction::Next
        } else {
            Direction::Previous
        };
        select_navigation_window(manager, &windows, process.as_deref(), direction)?
    };

    if !dry_run {
        manager.focus_window(selected).with_context(|| {
            format!(
                "failed to focus {} - {} (pid {}, window {})",
                selected.process, selected.title, selected.key.pid, selected.key.id
            )
        })?;
    }
    print_focus(selected, dry_run);
    Ok(())
}

#[derive(Clone, Copy)]
enum Direction {
    Next,
    Previous,
}

fn select_navigation_window<'a>(
    manager: &dyn WindowManager,
    windows: &'a [Window],
    process: Option<&str>,
    direction: Direction,
) -> Result<&'a Window> {
    let focused = manager.focused_window()?;
    let process_name = match process {
        Some(process) => process,
        None => focused
            .as_ref()
            .map(|window| window.process.as_str())
            .context("No focused window found.")?,
    };
    let candidates: Vec<_> = windows
        .iter()
        .filter(|window| {
            if process.is_some() {
                contains_case_insensitive(&window.process, process_name)
            } else {
                window.process.eq_ignore_ascii_case(process_name)
            }
        })
        .collect();
    if candidates.is_empty() {
        bail!("No matching windows found.");
    }

    let current = focused.as_ref().and_then(|focused| {
        candidates
            .iter()
            .position(|window| window.key == focused.key)
    });
    let index = match (direction, current) {
        (Direction::Next, Some(index)) => (index + 1) % candidates.len(),
        (Direction::Previous, Some(0)) => candidates.len() - 1,
        (Direction::Previous, Some(index)) => index - 1,
        (Direction::Next, None) => 0,
        (Direction::Previous, None) => candidates.len() - 1,
    };
    Ok(candidates[index])
}

fn print_focus(window: &Window, dry_run: bool) {
    println!(
        "{}:\n\nPID:      {}\nProcess:  {}\nTitle:    {}\nWindow:   {}",
        if dry_run { "Would focus" } else { "Focused" },
        window.key.pid,
        window.process,
        window.title,
        window.key.id
    );
}

fn inspect_profile(command: ProfileCommand, overrides: CliOverrides) -> Result<()> {
    let config = Config::load()?;
    match command {
        ProfileCommand::List => {
            for name in config.profiles.keys() {
                println!("{name}");
            }
        }
        ProfileCommand::Show { name } => {
            let profile = config
                .profiles
                .get(&name)
                .with_context(|| format!("Profile '{name}' not found."))?;
            let settings = resolve_settings(&config, Some(profile), overrides)?;
            println!("Profile: {name}\n");
            println!("process: {}", profile.process);
            println!("layout: {}", settings.layout);
            println!(
                "columns: {}",
                settings
                    .columns
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "automatic".into())
            );
            println!("gap: {}", settings.gap);
            println!("margin: {}", settings.margin);
            println!("sort: {}", settings.sort);
            if !profile.slots.is_empty() {
                println!("slots:");
                for rule in &profile.slots {
                    println!("  {}: {}", rule.position, rule.title);
                }
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct WindowListEntry<'a> {
    pid: u32,
    window_id: &'a str,
    process: &'a str,
    title: &'a str,
    monitor: &'a str,
    state: &'static str,
    bounds: Rect,
}

fn window_list_entries<'a>(
    windows: &'a [Window],
    config: &Config,
    state: &State,
) -> Vec<WindowListEntry<'a>> {
    windows
        .iter()
        .map(|window| WindowListEntry {
            pid: window.key.pid,
            window_id: &window.key.id,
            process: &window.process,
            title: &window.title,
            monitor: &window.monitor_id,
            state: window_state(window, config, state),
            bounds: window.bounds,
        })
        .collect()
}

fn window_state(window: &Window, config: &Config, state: &State) -> &'static str {
    if config.ignores_process(&window.process) {
        "ignored"
    } else if state.contains(&window.key) {
        "floating"
    } else {
        "managed"
    }
}

fn list_windows(manager: &dyn WindowManager, verbose: bool, json: bool) -> Result<()> {
    let config = Config::load()?;
    let state = State::load()?;
    let mut windows = manager.visible_windows()?;
    sort_windows(&mut windows, SortOrder::Title);

    if json {
        let entries = window_list_entries(&windows, &config, &state);
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if verbose {
        println!("PROCESS\tPID\tTITLE\tMONITOR\tSTATE\tWINDOW ID");
    } else {
        println!("PROCESS\tPID\tTITLE\tMONITOR\tSTATE");
    }
    for window in windows {
        let status = window_state(&window, &config, &state);
        if verbose {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                window.process,
                window.key.pid,
                window.title,
                window.monitor_id,
                status,
                window.key.id
            );
        } else {
            println!(
                "{}\t{}\t{}\t{}\t{}",
                window.process, window.key.pid, window.title, window.monitor_id, status
            );
        }
    }
    Ok(())
}

fn show_status(manager: &dyn WindowManager) -> Result<()> {
    let config = Config::load()?;
    let state = State::load()?;
    let windows = if state.floating.is_empty() {
        Vec::new()
    } else {
        manager.visible_windows()?
    };

    println!("Config:\n  {}", active_config_path()?.display());
    println!("\nProfiles:");
    if config.profiles.is_empty() {
        println!("  (none)");
    } else {
        for name in config.profiles.keys() {
            let default = if config.default_profile.as_deref() == Some(name.as_str()) {
                " (default)"
            } else {
                ""
            };
            println!("  {name}{default}");
        }
    }

    println!("\nFloating windows:");
    if state.floating.is_empty() {
        println!("  (none)");
    } else {
        for key in &state.floating {
            if let Some(window) = windows.iter().find(|window| &window.key == key) {
                println!("  {} - {}", window.process, window.title);
            } else {
                println!("  pid {} / window {} (not visible)", key.pid, key.id);
            }
        }
    }

    println!("\nIgnore rules:");
    if config.ignore.is_empty() {
        println!("  (none)");
    } else {
        for rule in &config.ignore {
            match rule {
                IgnoreRule::Process { value } => println!("  {value}"),
            }
        }
    }
    Ok(())
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::model::WindowKey;

    struct TestWindowManager {
        windows: Vec<Window>,
        current: Option<Window>,
        focused: RefCell<Vec<WindowKey>>,
    }

    impl WindowManager for TestWindowManager {
        fn visible_windows(&self) -> Result<Vec<Window>> {
            Ok(self.windows.clone())
        }

        fn focused_window(&self) -> Result<Option<Window>> {
            Ok(self.current.clone())
        }

        fn focus_window(&self, window: &Window) -> Result<()> {
            self.focused.borrow_mut().push(window.key.clone());
            Ok(())
        }

        fn monitors(&self) -> Result<Vec<Monitor>> {
            Ok(Vec::new())
        }

        fn set_bounds(&self, _: &Window, _: Rect) -> Result<()> {
            Ok(())
        }
    }

    fn window(process: &str, title: &str, pid: u32, monitor: &str) -> Window {
        Window {
            key: WindowKey {
                pid,
                id: pid.to_string(),
            },
            process: process.into(),
            title: title.into(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            monitor_id: monitor.into(),
        }
    }

    #[test]
    fn focused_selection_applies_ignore_and_floating_filters() {
        let unity_a = window("Unity", "A", 1, "1");
        let unity_b = window("Unity", "B", 2, "1");
        let rider = window("Rider", "Project", 3, "1");
        let mut state = State::default();
        state.insert(unity_b.key.clone());
        let selected = select_windows(
            vec![unity_a.clone(), unity_b, rider],
            &Selection::ProcessExact("unity".into()),
            &Config::default(),
            &state,
        );
        assert_eq!(selected, vec![unity_a]);

        let mut config = Config::default();
        config.add_process("Unity".into());
        assert!(select_windows(
            selected,
            &Selection::ProcessExact("Unity".into()),
            &config,
            &State::default(),
        )
        .is_empty());
    }

    #[test]
    fn focus_uses_pid_and_dry_run_does_not_change_focus() {
        let manager = TestWindowManager {
            windows: vec![
                window("Unity", "Editor", 10, "1"),
                window("Unity", "Editor", 20, "1"),
            ],
            current: None,
            focused: RefCell::new(Vec::new()),
        };

        let args = || FocusArgs {
            pid: Some(20),
            title: None,
            next: false,
            prev: false,
        };
        focus_window(&manager, args(), None, true).unwrap();
        assert!(manager.focused.borrow().is_empty());

        focus_window(&manager, args(), None, false).unwrap();
        assert_eq!(
            manager.focused.borrow().as_slice(),
            [WindowKey {
                pid: 20,
                id: "20".into(),
            }]
        );
    }

    #[test]
    fn title_focus_is_partial_case_insensitive_and_process_scoped() {
        let manager = TestWindowManager {
            windows: vec![
                window("Browser", "Client-02 docs", 30, "1"),
                window("Unity", "Client-02", 20, "1"),
                window("Unity", "client-01", 10, "1"),
            ],
            current: None,
            focused: RefCell::new(Vec::new()),
        };
        focus_window(
            &manager,
            FocusArgs {
                pid: None,
                title: Some("CLIENT".into()),
                next: false,
                prev: false,
            },
            Some("unity".into()),
            false,
        )
        .unwrap();
        assert_eq!(manager.focused.borrow()[0].pid, 10);
    }

    #[test]
    fn navigation_reuses_stable_order_and_wraps() {
        let server = window("Unity", "Server", 1, "1");
        let client_01 = window("Unity", "Client-01", 2, "1");
        let client_02 = window("Unity", "Client-02", 3, "1");
        let manager = TestWindowManager {
            windows: vec![server, client_02.clone(), client_01.clone()],
            current: Some(client_02),
            focused: RefCell::new(Vec::new()),
        };
        let mut windows = manager.visible_windows().unwrap();
        sort_windows(&mut windows, SortOrder::Title);

        let next = select_navigation_window(&manager, &windows, None, Direction::Next).unwrap();
        assert_eq!(next.title, "Server");
        let previous =
            select_navigation_window(&manager, &windows, None, Direction::Previous).unwrap();
        assert_eq!(previous, &client_01);
    }

    #[test]
    fn explicit_process_navigation_starts_at_an_end_when_focus_is_elsewhere() {
        let manager = TestWindowManager {
            windows: vec![
                window("Unity", "Server", 1, "1"),
                window("Unity", "Client", 2, "1"),
            ],
            current: Some(window("Rider", "Project", 3, "1")),
            focused: RefCell::new(Vec::new()),
        };
        let mut windows = manager.visible_windows().unwrap();
        sort_windows(&mut windows, SortOrder::Title);
        assert_eq!(
            select_navigation_window(&manager, &windows, Some("Unity"), Direction::Next)
                .unwrap()
                .title,
            "Client"
        );
        assert_eq!(
            select_navigation_window(&manager, &windows, Some("Unity"), Direction::Previous)
                .unwrap()
                .title,
            "Server"
        );
    }

    #[test]
    fn json_window_list_contains_pid_and_window_id() {
        let windows = vec![window("Unity", "Editor", 42, "1")];
        let entries = window_list_entries(&windows, &Config::default(), &State::default());
        let value = serde_json::to_value(entries).unwrap();

        assert_eq!(value[0]["pid"], 42);
        assert_eq!(value[0]["window_id"], "42");
        assert_eq!(value[0]["state"], "managed");
    }

    #[test]
    fn settings_precedence_is_cli_profile_global_default() {
        let config = Config {
            columns: Some(2),
            gap: Some(8),
            margin: Some(4),
            ..Config::default()
        };
        let profile = Profile {
            columns: Some(3),
            gap: Some(12),
            ..Profile::default()
        };
        let settings = resolve_settings(
            &config,
            Some(&profile),
            CliOverrides {
                columns: Some(4),
                gap: Some(16),
                margin: None,
            },
        )
        .unwrap();
        assert_eq!(settings.columns, Some(4));
        assert_eq!(settings.gap, 16);
        assert_eq!(settings.margin, 4);
        assert_eq!(settings.layout, LayoutKind::Grid);
        assert_eq!(settings.sort, SortOrder::Title);
    }

    #[test]
    fn ordering_and_multi_monitor_plans_are_stable_and_independent() {
        let mut windows = vec![
            window("Unity", "Client-02", 2, "2"),
            window("Unity", "Server", 3, "1"),
            window("Unity", "Client-01", 1, "2"),
        ];
        sort_windows(&mut windows, SortOrder::Title);
        assert_eq!(
            windows.iter().map(|w| w.title.as_str()).collect::<Vec<_>>(),
            vec!["Client-01", "Client-02", "Server"]
        );
        let monitors = vec![
            Monitor {
                id: "1".into(),
                work_area: Rect {
                    x: 0,
                    y: 0,
                    width: 1000,
                    height: 800,
                },
            },
            Monitor {
                id: "2".into(),
                work_area: Rect {
                    x: 1000,
                    y: 0,
                    width: 1000,
                    height: 800,
                },
            },
        ];
        let plans = build_plan(
            &windows,
            &monitors,
            EffectiveSettings {
                layout: LayoutKind::Grid,
                sort: SortOrder::Title,
                columns: None,
                gap: 0,
                margin: 0,
            },
            &[],
        )
        .unwrap();
        assert_eq!(plans.len(), 3);
        assert_eq!(plans.iter().filter(|p| p.monitor.id == "1").count(), 1);
        assert_eq!(plans.iter().filter(|p| p.monitor.id == "2").count(), 2);
        assert!(plans
            .iter()
            .filter(|p| p.monitor.id == "2")
            .all(|p| p.bounds.x >= 1000));
    }

    #[test]
    fn slot_rules_pin_matches_and_fill_remaining_slots_stably() {
        let mut windows = vec![
            window("Unity", "Client-02", 3, "1"),
            window("Unity", "Server", 1, "1"),
            window("Unity", "Client-01", 2, "1"),
        ];
        sort_windows(&mut windows, SortOrder::Title);
        let assigned = assign_slots(
            windows.iter().collect(),
            &[SlotRule {
                title: "Server".into(),
                position: 0,
            }],
        );
        assert_eq!(
            assigned
                .iter()
                .map(|(slot, window)| (*slot, window.title.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "Server"), (1, "Client-01"), (2, "Client-02")]
        );
    }

    #[test]
    fn unmatched_slot_rules_do_not_reserve_space() {
        let windows = [window("Unity", "Server", 1, "1")];
        let assigned = assign_slots(
            windows.iter().collect(),
            &[SlotRule {
                title: "Crashed Client".into(),
                position: 7,
            }],
        );
        assert_eq!(assigned[0].0, 0);
    }

    #[test]
    fn matched_sparse_slot_preserves_its_grid_position() {
        let mut windows = vec![
            window("Unity", "Server", 1, "1"),
            window("Unity", "Client", 2, "1"),
        ];
        sort_windows(&mut windows, SortOrder::Title);
        let monitor = Monitor {
            id: "1".into(),
            work_area: Rect {
                x: 0,
                y: 0,
                width: 1000,
                height: 800,
            },
        };
        let monitors = [monitor];
        let plans = build_plan(
            &windows,
            &monitors,
            EffectiveSettings {
                layout: LayoutKind::Grid,
                sort: SortOrder::Title,
                columns: Some(2),
                gap: 0,
                margin: 0,
            },
            &[SlotRule {
                title: "Server".into(),
                position: 3,
            }],
        )
        .unwrap();
        let server = plans
            .iter()
            .find(|plan| plan.window.title == "Server")
            .unwrap();
        assert_eq!(server.slot, 3);
        assert_eq!(server.bounds.x, 500);
        assert_eq!(server.bounds.y, 400);
    }
}
