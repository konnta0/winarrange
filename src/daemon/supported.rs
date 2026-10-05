use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
#[cfg(target_os = "macos")]
use interprocess::local_socket::GenericFilePath;
#[cfg(target_os = "windows")]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{prelude::*, ListenerOptions, Stream as LocalSocketStream};
use serde::{Deserialize, Serialize};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

use crate::{
    action::Action,
    app,
    cli::DaemonCommand,
    config::{self, Config, HotkeyBinding},
    platform::{self, WindowManager},
};

use super::autostart;

pub fn command(command: Option<DaemonCommand>) -> Result<()> {
    match command.unwrap_or(DaemonCommand::Run { background: false }) {
        DaemonCommand::Start => start(),
        DaemonCommand::Stop => request_and_print(Request::Stop),
        DaemonCommand::Status => status(),
        DaemonCommand::Restart => restart(),
        DaemonCommand::Reload => request_and_print(Request::Reload),
        DaemonCommand::Install => {
            autostart::install()?;
            println!("winarrange daemon installed.");
            Ok(())
        }
        DaemonCommand::Uninstall => {
            autostart::uninstall()?;
            println!("winarrange daemon uninstalled.");
            Ok(())
        }
        DaemonCommand::Run { background } => {
            let result = run(!background);
            if background {
                if let Err(error) = &result {
                    let _ =
                        Logger { foreground: false }.log(&format!("daemon fatal error: {error:#}"));
                }
            }
            result
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Request {
    Status,
    Stop,
    Reload,
}

impl Request {
    fn wire(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Stop => "stop",
            Self::Reload => "reload",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "status" => Some(Self::Status),
            "stop" => Some(Self::Stop),
            "reload" => Some(Self::Reload),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    ok: bool,
    message: String,
}

impl Response {
    fn success(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }

    fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

fn status() -> Result<()> {
    match send_request(Request::Status) {
        Ok(response) if response.ok => println!("{}", response.message),
        Ok(response) => bail!(response.message),
        Err(_) => println!("winarrange daemon is not running"),
    }
    Ok(())
}

fn request_and_print(request: Request) -> Result<()> {
    let response = send_request(request).with_context(|| "winarrange daemon is not running")?;
    if response.ok {
        println!("{}", response.message);
        Ok(())
    } else {
        bail!(response.message)
    }
}

fn start() -> Result<()> {
    if let Ok(response) = send_request(Request::Status) {
        if response.ok {
            println!(
                "winarrange daemon is already running.\n{}",
                response.message
            );
            return Ok(());
        }
    }

    let executable = std::env::current_exe().context("failed to locate winarrange executable")?;
    let mut child = Command::new(executable);
    child
        .args(["daemon", "run", "--background"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::process::CommandExt;

        extern "C" {
            fn setsid() -> i32;
        }
        unsafe {
            child.pre_exec(|| {
                if setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        child.creation_flags(0x0800_0000);
    }
    child.spawn().context("failed to start winarrange daemon")?;

    for _ in 0..40 {
        thread::sleep(Duration::from_millis(50));
        if let Ok(response) = send_request(Request::Status) {
            if response.ok {
                println!("winarrange daemon started.\n{}", response.message);
                return Ok(());
            }
        }
    }
    bail!(
        "winarrange daemon did not become ready; inspect {}",
        log_path()?.display()
    )
}

fn restart() -> Result<()> {
    if send_request(Request::Status).is_ok() {
        request_and_print(Request::Stop)?;
        for _ in 0..40 {
            if send_request(Request::Status).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    start()
}

fn run(foreground: bool) -> Result<()> {
    let listener = create_listener().map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!("winarrange daemon is already running")
        } else {
            anyhow::Error::from(error).context("failed to create daemon IPC endpoint")
        }
    })?;

    let logger = Logger { foreground };
    logger.log("daemon start")?;
    let config = Config::load().context("failed to load daemon configuration")?;
    logger.log(&format!("config load: {} hotkey(s)", config.hotkeys.len()))?;

    let mut event_loop_builder = EventLoop::<DaemonEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    event_loop_builder
        .with_activation_policy(ActivationPolicy::Accessory)
        .with_default_menu(false)
        .with_activate_ignoring_other_apps(false);
    let event_loop = event_loop_builder
        .build()
        .context("failed to create daemon event loop")?;
    let proxy = event_loop.create_proxy();

    GlobalHotKeyEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(DaemonEvent::Hotkey(event));
        }
    }));

    let mut daemon = DaemonApp::new(config, logger)?;
    spawn_ipc_server(listener, proxy);
    event_loop
        .run_app(&mut daemon)
        .context("daemon event loop failed")?;
    Ok(())
}

#[derive(Debug)]
enum DaemonEvent {
    Hotkey(GlobalHotKeyEvent),
    Control {
        request: Request,
        reply: mpsc::Sender<Response>,
    },
}

struct RegisteredBinding {
    config: HotkeyBinding,
    native: HotKey,
}

struct DaemonApp {
    hotkeys: GlobalHotKeyManager,
    bindings: Vec<RegisteredBinding>,
    action_queue: VecDeque<Action>,
    manager: Box<dyn WindowManager>,
    logger: Logger,
}

impl DaemonApp {
    fn new(config: Config, logger: Logger) -> Result<Self> {
        let hotkeys = GlobalHotKeyManager::new().context("failed to initialize global hotkeys")?;
        let bindings = resolve_bindings(&config)?;
        register_all(&hotkeys, &bindings)?;
        for binding in &bindings {
            logger.log(&format!(
                "hotkey registered: {} -> {}",
                binding.config.keys.display, binding.config.action
            ))?;
        }
        Ok(Self {
            hotkeys,
            bindings,
            action_queue: VecDeque::new(),
            manager: platform::system_window_manager()?,
            logger,
        })
    }

    fn reload(&mut self) -> Result<()> {
        let config = Config::load().context("new config is invalid; keeping existing hotkeys")?;
        let new_bindings = resolve_bindings(&config)
            .context("new hotkey config is invalid; keeping existing hotkeys")?;
        replace_registrations(&self.hotkeys, &mut self.bindings, new_bindings)?;
        self.logger.log("reload: hotkeys replaced")?;
        Ok(())
    }

    fn status_text(&self) -> String {
        let mut output = format!("Status: running\nPID: {}\n\nHotkeys:", std::process::id());
        if self.bindings.is_empty() {
            output.push_str("\n  (none configured)");
        } else {
            for binding in &self.bindings {
                output.push_str(&format!(
                    "\n  {:<24} {}",
                    binding.config.keys.display, binding.config.action
                ));
            }
        }
        output
    }

    fn execute(&self, action: &Action) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            app::execute_action(action, self.manager.as_ref())
        }));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = self
                    .logger
                    .log(&format!("action execution failure ({action}): {error:#}"));
            }
            Err(_) => {
                let _ = self.logger.log(&format!(
                    "action execution panic ({action}); daemon continues"
                ));
            }
        }
    }
}

impl ApplicationHandler<DaemonEvent> for DaemonApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: DaemonEvent) {
        match event {
            DaemonEvent::Hotkey(event) if event.state == HotKeyState::Pressed => {
                let action = self
                    .bindings
                    .iter()
                    .find(|binding| binding.native.id() == event.id)
                    .map(|binding| binding.config.action.clone());
                if let Some(action) = action {
                    self.action_queue.push_back(action);
                    while let Some(action) = self.action_queue.pop_front() {
                        self.execute(&action);
                    }
                }
            }
            DaemonEvent::Hotkey(_) => {}
            DaemonEvent::Control { request, reply } => match request {
                Request::Status => {
                    let _ = reply.send(Response::success(self.status_text()));
                }
                Request::Reload => {
                    let response = match self.reload() {
                        Ok(()) => Response::success("winarrange daemon reloaded."),
                        Err(error) => {
                            let _ = self.logger.log(&format!("reload failure: {error:#}"));
                            Response::failure(format!("Failed to reload daemon: {error:#}"))
                        }
                    };
                    let _ = reply.send(response);
                }
                Request::Stop => {
                    let _ = self.logger.log("daemon stop");
                    let _ = reply.send(Response::success("winarrange daemon stopped."));
                    event_loop.exit();
                }
            },
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        for binding in &self.bindings {
            let _ = self.hotkeys.unregister(binding.native);
        }
    }
}

fn resolve_bindings(config: &Config) -> Result<Vec<RegisteredBinding>> {
    config
        .hotkey_bindings()?
        .into_iter()
        .map(|config| {
            let native =
                config.keys.normalized.parse::<HotKey>().with_context(|| {
                    format!("invalid platform hotkey '{}':", config.keys.display)
                })?;
            Ok(RegisteredBinding { config, native })
        })
        .collect()
}

trait HotkeyRegistrar {
    fn register_hotkey(&self, hotkey: HotKey) -> Result<()>;
    fn unregister_hotkey(&self, hotkey: HotKey) -> Result<()>;
}

impl HotkeyRegistrar for GlobalHotKeyManager {
    fn register_hotkey(&self, hotkey: HotKey) -> Result<()> {
        self.register(hotkey).map_err(Into::into)
    }

    fn unregister_hotkey(&self, hotkey: HotKey) -> Result<()> {
        self.unregister(hotkey).map_err(Into::into)
    }
}

fn register_all(manager: &impl HotkeyRegistrar, bindings: &[RegisteredBinding]) -> Result<()> {
    let mut registered = Vec::new();
    for binding in bindings {
        if let Err(error) = manager.register_hotkey(binding.native) {
            for native in registered {
                let _ = manager.unregister_hotkey(native);
            }
            bail!(
                "Failed to register hotkey:\n\n  {}\n\nAction:\n  {}\n\nThe hotkey may already be registered by another application.\n\nPlatform error: {error}",
                binding.config.keys.display,
                binding.config.action
            );
        }
        registered.push(binding.native);
    }
    Ok(())
}

fn replace_registrations(
    manager: &impl HotkeyRegistrar,
    current: &mut Vec<RegisteredBinding>,
    replacement: Vec<RegisteredBinding>,
) -> Result<()> {
    let mut removed = Vec::new();
    for binding in current.iter() {
        if let Err(error) = manager.unregister_hotkey(binding.native) {
            for native in removed {
                let _ = manager.register_hotkey(native);
            }
            return Err(error).with_context(|| {
                format!(
                    "failed to unregister hotkey '{}'; previous hotkeys restored",
                    binding.config.keys.display
                )
            });
        }
        removed.push(binding.native);
    }

    if let Err(error) = register_all(manager, &replacement) {
        for binding in &replacement {
            let _ = manager.unregister_hotkey(binding.native);
        }
        let rollback = register_all(manager, current);
        if let Err(rollback_error) = rollback {
            return Err(error.context(format!(
                "rollback of previous hotkeys also failed: {rollback_error:#}"
            )));
        }
        return Err(error.context("new hotkeys were rejected; previous hotkeys restored"));
    }

    *current = replacement;
    Ok(())
}

fn spawn_ipc_server(
    listener: interprocess::local_socket::Listener,
    proxy: winit::event_loop::EventLoopProxy<DaemonEvent>,
) {
    thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(connection) = connection else {
                continue;
            };
            let mut connection = BufReader::new(connection);
            let mut line = String::new();
            let response = match connection.read_line(&mut line) {
                Ok(0) => Response::failure("empty daemon request"),
                Ok(_) => match Request::parse(&line) {
                    Some(request) => {
                        let (sender, receiver) = mpsc::channel();
                        if proxy
                            .send_event(DaemonEvent::Control {
                                request,
                                reply: sender,
                            })
                            .is_err()
                        {
                            Response::failure("daemon is stopping")
                        } else {
                            receiver
                                .recv()
                                .unwrap_or_else(|_| Response::failure("daemon did not respond"))
                        }
                    }
                    None => Response::failure(format!("unknown daemon request: {}", line.trim())),
                },
                Err(error) => Response::failure(format!("failed to read daemon request: {error}")),
            };
            if let Ok(mut json) = serde_json::to_vec(&response) {
                json.push(b'\n');
                let _ = connection.get_mut().write_all(&json);
            }
        }
    });
}

fn send_request(request: Request) -> Result<Response> {
    let mut connection = BufReader::new(connect_ipc().context("failed to connect to daemon")?);
    connection
        .get_mut()
        .write_all(format!("{}\n", request.wire()).as_bytes())?;
    let mut response = String::new();
    connection.read_line(&mut response)?;
    if response.is_empty() {
        bail!("daemon closed the IPC connection without a response");
    }
    serde_json::from_str(&response).context("daemon returned an invalid response")
}

#[cfg(target_os = "macos")]
fn ipc_path() -> Result<std::path::PathBuf> {
    Ok(config::data_root()?.join("daemon.sock"))
}

#[cfg(target_os = "macos")]
fn bind_path(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Listener> {
    let name = path.to_path_buf().to_fs_name::<GenericFilePath>()?;
    ListenerOptions::new().name(name).create_sync()
}

#[cfg(target_os = "macos")]
fn create_listener() -> std::io::Result<interprocess::local_socket::Listener> {
    let path = ipc_path().map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match bind_path(&path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            // Only reclaim the exact per-user socket after a connection
            // attempt proves the previous daemon is gone.
            if connect_ipc().is_ok() {
                return Err(error);
            }
            std::fs::remove_file(&path)?;
            bind_path(&path)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "windows")]
fn create_listener() -> std::io::Result<interprocess::local_socket::Listener> {
    let endpoint = endpoint_name().map_err(std::io::Error::other)?;
    let name = endpoint.to_ns_name::<GenericNamespaced>()?;
    ListenerOptions::new().name(name).create_sync()
}

#[cfg(target_os = "macos")]
fn connect_ipc() -> std::io::Result<LocalSocketStream> {
    let name = ipc_path()
        .map_err(std::io::Error::other)?
        .to_fs_name::<GenericFilePath>()?;
    LocalSocketStream::connect(name)
}

#[cfg(target_os = "windows")]
fn connect_ipc() -> std::io::Result<LocalSocketStream> {
    let endpoint = endpoint_name().map_err(std::io::Error::other)?;
    let name = endpoint.to_ns_name::<GenericNamespaced>()?;
    LocalSocketStream::connect(name)
}

#[cfg(target_os = "windows")]
fn endpoint_name() -> Result<String> {
    let root = config::data_root()?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in root.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("winarrange-daemon-{hash:016x}"))
}

fn log_path() -> Result<std::path::PathBuf> {
    Ok(config::data_root()?.join("daemon.log"))
}

struct Logger {
    foreground: bool,
}

impl Logger {
    fn log(&self, message: &str) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let line = format!("[{timestamp}] {message}\n");
        if self.foreground {
            eprint!("{line}");
        }
        let path = log_path()?;
        let parent = path.parent().context("invalid daemon log path")?;
        std::fs::create_dir_all(parent)?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?
            .write_all(line.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeSet};

    use super::*;

    #[derive(Default)]
    struct FakeRegistrar {
        registered: RefCell<BTreeSet<u32>>,
        reject: RefCell<Option<u32>>,
    }

    impl HotkeyRegistrar for FakeRegistrar {
        fn register_hotkey(&self, hotkey: HotKey) -> Result<()> {
            if *self.reject.borrow() == Some(hotkey.id()) {
                bail!("simulated collision");
            }
            self.registered.borrow_mut().insert(hotkey.id());
            Ok(())
        }

        fn unregister_hotkey(&self, hotkey: HotKey) -> Result<()> {
            self.registered.borrow_mut().remove(&hotkey.id());
            Ok(())
        }
    }

    fn binding(keys: &str, action: Action) -> RegisteredBinding {
        let keys = crate::action::HotkeySpec::parse(keys).unwrap();
        let native = keys.normalized.parse().unwrap();
        RegisteredBinding {
            config: HotkeyBinding { keys, action },
            native,
        }
    }

    #[test]
    fn endpoint_is_stable_and_user_specific() {
        #[cfg(target_os = "macos")]
        {
            let first = ipc_path().unwrap();
            assert_eq!(first, ipc_path().unwrap());
            assert!(first.ends_with("winarrange/daemon.sock"));
        }
        #[cfg(target_os = "windows")]
        {
            let first = endpoint_name().unwrap();
            assert_eq!(first, endpoint_name().unwrap());
            assert!(first.starts_with("winarrange-daemon-"));
        }
    }

    #[test]
    fn wire_requests_round_trip() {
        for request in [Request::Status, Request::Stop, Request::Reload] {
            assert!(Request::parse(request.wire()).is_some());
        }
    }

    #[test]
    fn reload_adds_removes_and_changes_hotkeys() {
        let manager = FakeRegistrar::default();
        let mut current = vec![
            binding("ctrl+a", Action::Arrange),
            binding("ctrl+b", Action::FocusNext),
        ];
        register_all(&manager, &current).unwrap();
        let replacement = vec![
            binding("ctrl+b", Action::FocusPrevious),
            binding("ctrl+c", Action::ToggleFloat),
        ];
        replace_registrations(&manager, &mut current, replacement).unwrap();

        let expected = current
            .iter()
            .map(|binding| binding.native.id())
            .collect::<BTreeSet<_>>();
        assert_eq!(*manager.registered.borrow(), expected);
        assert_eq!(current[0].config.action, Action::FocusPrevious);
    }

    #[test]
    fn failed_reload_restores_previous_hotkeys() {
        let manager = FakeRegistrar::default();
        let mut current = vec![binding("ctrl+a", Action::Arrange)];
        register_all(&manager, &current).unwrap();
        let original = manager.registered.borrow().clone();
        let replacement = vec![binding("ctrl+b", Action::FocusNext)];
        *manager.reject.borrow_mut() = Some(replacement[0].native.id());

        assert!(replace_registrations(&manager, &mut current, replacement).is_err());
        assert_eq!(*manager.registered.borrow(), original);
        assert_eq!(current[0].config.action, Action::Arrange);
    }

    #[test]
    fn action_queue_is_fifo_and_drained_one_at_a_time() {
        let mut queue = VecDeque::from([Action::Arrange, Action::FocusNext, Action::ToggleFloat]);
        let mut executed = Vec::new();
        while let Some(action) = queue.pop_front() {
            executed.push(action);
        }
        assert_eq!(
            executed,
            vec![Action::Arrange, Action::FocusNext, Action::ToggleFloat,]
        );
    }

    #[test]
    fn second_listener_cannot_take_the_same_instance_name() {
        let unique = format!(
            "winarrange-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        #[cfg(target_os = "macos")]
        let first_name = std::env::temp_dir()
            .join(format!("{unique}.sock"))
            .to_fs_name::<GenericFilePath>()
            .unwrap();
        #[cfg(target_os = "windows")]
        let first_name = unique.clone().to_ns_name::<GenericNamespaced>().unwrap();
        let _first = ListenerOptions::new()
            .name(first_name)
            .create_sync()
            .unwrap();
        #[cfg(target_os = "macos")]
        let second_name = std::env::temp_dir()
            .join(format!("{unique}.sock"))
            .to_fs_name::<GenericFilePath>()
            .unwrap();
        #[cfg(target_os = "windows")]
        let second_name = unique.to_ns_name::<GenericNamespaced>().unwrap();
        let error = ListenerOptions::new()
            .name(second_name)
            .create_sync()
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    }
}
