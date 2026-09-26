use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};

use crate::{
    cli::{Cli, Command, IgnoreCommand},
    config::{Config, IgnoreRule},
    layout,
    model::{Monitor, Rect, Window},
    platform::WindowManager,
    state::State,
};

pub fn run(cli: Cli, manager: &dyn WindowManager) -> Result<()> {
    match cli.command {
        None => arrange_windows(manager, cli.process.as_deref(), cli.dry_run),
        Some(Command::Float) => change_float(manager, FloatAction::Add),
        Some(Command::Unfloat) => change_float(manager, FloatAction::Remove),
        Some(Command::ToggleFloat) => change_float(manager, FloatAction::Toggle),
        Some(Command::Ignore { command }) => change_ignore(command),
    }
}

fn arrange_windows(
    manager: &dyn WindowManager,
    process_filter: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let config = Config::load()?;
    let state = State::load()?;
    let all_windows = manager.visible_windows()?;

    let mut windows: Vec<_> = all_windows
        .into_iter()
        .filter(|window| {
            process_filter.is_none_or(|needle| contains_case_insensitive(&window.process, needle))
        })
        .filter(|window| !config.ignores_process(&window.process))
        .filter(|window| !state.contains(&window.key))
        .collect();

    windows.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.process.to_lowercase().cmp(&b.process.to_lowercase()))
            .then_with(|| a.key.pid.cmp(&b.key.pid))
            .then_with(|| a.key.id.cmp(&b.key.id))
    });

    let monitors = manager.monitors()?;
    let monitor_map: BTreeMap<_, _> = monitors.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut groups: BTreeMap<&str, Vec<&Window>> = BTreeMap::new();
    for window in &windows {
        if monitor_map.contains_key(window.monitor_id.as_str()) {
            groups.entry(&window.monitor_id).or_default().push(window);
        }
    }

    if windows.is_empty() {
        println!("No matching windows.");
        return Ok(());
    }

    for (monitor_id, group) in groups {
        let monitor = monitor_map[monitor_id];
        for (window, bounds) in group
            .iter()
            .zip(layout::arrange(group.len(), monitor.work_area))
        {
            if dry_run {
                print_plan(window, monitor, bounds);
            } else {
                manager.set_bounds(window, bounds).with_context(|| {
                    format!("failed to move {} - {}", window.process, window.title)
                })?;
            }
        }
    }
    Ok(())
}

fn print_plan(window: &Window, monitor: &Monitor, bounds: Rect) {
    println!(
        "{} - {}\n  monitor: {}\n  x: {}\n  y: {}\n  width: {}\n  height: {}\n",
        window.process, window.title, monitor.id, bounds.x, bounds.y, bounds.width, bounds.height
    );
}

enum FloatAction {
    Add,
    Remove,
    Toggle,
}

fn change_float(manager: &dyn WindowManager, action: FloatAction) -> Result<()> {
    let focused = manager
        .focused_window()?
        .context("no focused arrangeable window")?;
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
    state.save()?;
    println!(
        "{} - {}: {}",
        focused.process,
        focused.title,
        if now_floating { "floating" } else { "tiled" }
    );
    Ok(())
}

fn change_ignore(command: IgnoreCommand) -> Result<()> {
    let mut config = Config::load()?;
    match command {
        IgnoreCommand::Add { process } => {
            if process.trim().is_empty() {
                bail!("process name cannot be empty");
            }
            if config.add_process(process.clone()) {
                config.save()?;
                println!("Ignored process: {process}");
            } else {
                println!("Process is already ignored: {process}");
            }
        }
        IgnoreCommand::Remove { process } => {
            if config.remove_process(&process) {
                config.save()?;
                println!("Removed ignore rule: {process}");
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

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_filter_is_case_insensitive() {
        assert!(contains_case_insensitive("Unity Editor", "unity"));
        assert!(!contains_case_insensitive("Calculator", "Unity"));
    }
}
