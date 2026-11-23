//! ShitShooter: a fast free-for-all arena shooter with self-hosted lobbies.
//!
//! Run with no arguments for the menu, or see `--help`.

// Release builds are a Windows GUI app, so no console window opens next to the
// game. Debug builds (`cargo run`) keep the console for the log output.
// The modes that print text (`--server`, `--help`, `--selftest`) attach to a
// console themselves; see `attach_console`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod combat;
mod devtools;
mod grenade;
mod gun_model;
mod map;
mod net;
mod ping;
mod player;
mod protocol;
mod remote;
mod server;
mod settings;
mod ui;
mod weapons;
mod world;

use std::sync::atomic::AtomicBool;

use bevy::prelude::*;

use crate::server::{Server, ServerSettings};

/// The release build name (e.g. `build-42`) when built by the GitHub release
/// workflow, otherwise the Cargo version. Shown in the log and crash message.
pub const VERSION: &str = match option_env!("SHITSHOOTER_BUILD") {
    Some(build) => build,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    /// Waiting for the server to accept us.
    Connecting,
    Lobby,
    Playing,
}

/// Command line options.
#[derive(Resource, Clone, Default)]
pub struct LaunchArgs {
    pub dedicated: bool,
    pub host: bool,
    pub join: Option<String>,
    pub name: Option<String>,
    pub server: ServerSettings,
    /// Hidden developer option: play a scripted match and save screenshots here.
    pub selftest: Option<String>,
}

const HELP: &str = "\
ShitShooter

USAGE:
    shitshooter [OPTIONS]

With no options the game opens on the main menu.

OPTIONS:
    --host               Host a lobby right away (skips the menu)
    --join <ADDR[:PORT]> Join a lobby right away (skips the menu)
    --name <NAME>        Player name
    --server             Run a dedicated server with no window
    --port <PORT>        UDP port to host on              [default: 7777]
    --password <TEXT>    Lobby password (host and join)
    --mode <NAME>        Game mode: ffa, teams or hill    [default: ffa]
    --frags <N>          Kills needed to win (doubled for teams) [default: 20]
    --minutes <N>        Match time limit                 [default: 10]
    --bots <N>           Start with N bots
    --map <NAME>         Map to start on: arena or depot  [default: arena]
    --help               Show this text
";

fn parse_args() -> Result<LaunchArgs, String> {
    let mut args = LaunchArgs::default();
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let mut value = |what: &str| {
            iter.next()
                .ok_or_else(|| format!("{flag} needs a value ({what})"))
        };
        fn number<T: std::str::FromStr>(flag: &str, text: String) -> Result<T, String> {
            text.parse()
                .map_err(|_| format!("'{text}' is not a valid value for {flag}"))
        }
        match flag.as_str() {
            "--server" => args.dedicated = true,
            "--host" => args.host = true,
            "--join" => args.join = Some(value("address")?),
            "--name" => args.name = Some(value("name")?),
            "--password" => args.server.password = value("text")?,
            "--port" => args.server.port = number(&flag, value("port")?)?,
            "--frags" => args.server.frag_limit = number::<u32>(&flag, value("count")?)?.max(1),
            "--minutes" => args.server.match_minutes = number::<f32>(&flag, value("minutes")?)?.max(0.5),
            "--bots" => args.server.bots = number(&flag, value("count")?)?,
            "--mode" => {
                let name = value("name")?;
                args.server.mode = ["ffa", "teams", "hill"]
                    .iter()
                    .position(|known| known.eq_ignore_ascii_case(&name))
                    .ok_or_else(|| format!("unknown mode '{name}' (try: ffa, teams, hill)"))? as u8;
            }
            "--map" => {
                let name = value("name")?;
                args.server.map = map::MAP_NAMES
                    .iter()
                    .position(|known| known.eq_ignore_ascii_case(&name))
                    .ok_or_else(|| format!("unknown map '{name}' (try: arena, depot)"))? as u8;
            }
            "--selftest" => args.selftest = Some(value("directory")?),
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(args)
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            if attach_console(false) {
                if !message.is_empty() {
                    eprintln!("error: {message}\n");
                }
                print!("{HELP}");
            } else {
                // Started without a terminal (e.g. a shortcut with bad options).
                let error = if message.is_empty() { String::new() } else { format!("error: {message}\n\n") };
                show_error_box(&format!("{error}{HELP}"));
            }
            return;
        }
    };

    if args.selftest.is_some() {
        attach_console(false);
    }

    if args.dedicated {
        // The server has no game window, so it always needs a console: the
        // terminal it was started from, or a new window.
        attach_console(true);
        match Server::bind(args.server.clone()) {
            Ok(server) => {
                println!(
                    "[server] dedicated server listening on UDP port {} (Ctrl+C to stop)",
                    args.server.port
                );
                server.run(&AtomicBool::new(false));
            }
            Err(err) => eprintln!("error: could not bind UDP port {}: {err}", args.server.port),
        }
        return;
    }

    open_log_file();
    install_crash_reporter();

    let exit = App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "ShitShooter".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    custom_layer: log_file_layer,
                    ..default()
                }),
        )
        .insert_resource(args)
        .init_state::<AppState>()
        .add_plugins((
            net::NetPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            remote::RemotePlugin,
            combat::CombatPlugin,
            grenade::GrenadePlugin,
            ping::PingPlugin,
            gun_model::GunModelPlugin,
            // Before the UI, which starts from the saved settings.
            settings::SettingsPlugin,
            ui::UiPlugin,
            audio::AudioPlugin,
            devtools::DevToolsPlugin,
        ))
        .run();
    log_line(&format!("exited: {exit:?}"));
}

/// `shitshooter.log`, written next to the exe on every run. When the game will
/// not start on someone's PC, this file says how far it got and what went wrong.
static LOG_FILE: std::sync::OnceLock<(std::path::PathBuf, std::sync::Arc<std::fs::File>)> =
    std::sync::OnceLock::new();

fn open_log_file() {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("shitshooter.log")));
    // Fall back to the temp folder if the exe sits somewhere read-only.
    let candidates = beside_exe
        .into_iter()
        .chain([std::env::temp_dir().join("shitshooter.log")]);
    for path in candidates {
        if let Ok(file) = std::fs::File::create(&path) {
            let _ = LOG_FILE.set((path, std::sync::Arc::new(file)));
            break;
        }
    }
    log_line(&format!(
        "ShitShooter {} starting on {} {}",
        VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
}

fn log_line(text: &str) {
    use std::io::Write;
    if let Some((_, file)) = LOG_FILE.get() {
        let _ = writeln!(&**file, "{text}");
    }
}

/// Copies everything Bevy logs (GPU chosen, warnings, errors) into the log file.
fn log_file_layer(_app: &mut App) -> Option<bevy::log::BoxedLayer> {
    use bevy::log::tracing_subscriber::{Layer, fmt};
    let (_, file) = LOG_FILE.get()?;
    Some(fmt::layer().with_ansi(false).with_writer(file.clone()).boxed())
}

/// When the game crashes, say so: without this a double-clicked exe just
/// vanishes. The reason is shown in a message box and written to the log file.
fn install_crash_reporter() {
    use std::sync::atomic::Ordering;

    static REPORTED: AtomicBool = AtomicBool::new(false);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        if REPORTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut message = format!("ShitShooter {} crashed.\n\n{info}\n", VERSION);
        log_line(&format!("CRASH: {info}"));
        if let Some((path, _)) = LOG_FILE.get() {
            message.push_str(&format!("\nDetails were saved to:\n{}", path.display()));
        }
        message.push_str("\n\nIf it mentions the GPU, adapter or surface, update your graphics drivers.");
        show_error_box(&message);
        std::process::exit(1);
    }));
}

#[cfg(windows)]
fn show_error_box(text: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut std::ffi::c_void, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (text, caption) = (wide(text), wide("ShitShooter"));
    const MB_ICONERROR: u32 = 0x10;
    // SAFETY: both strings are valid, null-terminated UTF-16 that outlive the call.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONERROR);
    }
}

#[cfg(not(windows))]
fn show_error_box(_text: &str) {}

/// Makes `println!` reach a console in release builds, which have none of
/// their own. Uses the console of the terminal that started the game; failing
/// that, opens a new console window if `open_new` is set. Returns whether there
/// is somewhere for text to go (always true in debug builds and off Windows).
#[cfg(all(windows, not(debug_assertions)))]
fn attach_console(open_new: bool) -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn AllocConsole() -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 calls without pointers; they fail harmlessly if the
    // process already has a console.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 || (open_new && AllocConsole() != 0) }
}

#[cfg(not(all(windows, not(debug_assertions))))]
fn attach_console(_open_new: bool) -> bool {
    true
}
