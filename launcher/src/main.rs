//! ShitShooter launcher: installs the newest game build from the GitHub
//! releases, then starts it. Friends only ever download this once.
//!
//! The game lives in `%LOCALAPPDATA%\ShitShooter`, so the launcher itself can
//! sit anywhere (desktop, downloads). Without internet it starts whatever is
//! installed. Any arguments are passed on to the game (e.g. `--join <addr>`).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const REPO: &str = "badubies/ShitShooter";
const GAME_EXE: &str = "shitshooter.exe";
const LAUNCHER_EXE: &str = "ShitShooterLauncher.exe";
/// Release asset holding the game build name (e.g. `build-42`).
const GAME_VERSION_FILE: &str = "version.txt";
/// Release asset holding the launcher version, for the launcher's self-update.
const LAUNCHER_VERSION_FILE: &str = "launcher-version.txt";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Set on the freshly updated launcher so it never tries to update itself again
/// in the same run (guards against a loop if a release is ever inconsistent).
const JUST_UPDATED_ENV: &str = "SHITSHOOTER_LAUNCHER_UPDATED";

type Result<T> = std::result::Result<T, String>;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        // The release workflow uses this to write launcher-version.txt.
        println!("{VERSION}");
        return;
    }
    println!("ShitShooter launcher {VERSION}\n");
    if let Err(message) = run(&args) {
        println!("\n{message}");
        pause();
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent("ShitShooter-Launcher")
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .build()
        .into();

    if std::env::var_os(JUST_UPDATED_ENV).is_none()
        && let Err(e) = update_launcher(&agent, args)
    {
        println!("Could not update the launcher ({e}). Carrying on.\n");
    }
    remove_old_launcher();

    let dir = install_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let game = dir.join(GAME_EXE);
    let installed = game
        .exists()
        .then(|| fs::read_to_string(dir.join(GAME_VERSION_FILE)).ok())
        .flatten()
        .map(|v| v.trim().to_string());

    println!("Checking for updates...");
    match fetch_text(&agent, GAME_VERSION_FILE) {
        Ok(latest) if installed.as_deref() == Some(latest.as_str()) => {
            println!("Up to date ({latest}).");
        }
        Ok(latest) => {
            match &installed {
                Some(old) => println!("Updating {old} -> {latest}"),
                None => println!("Installing {latest}"),
            }
            if let Err(e) = install_game(&agent, &dir, &latest) {
                if !game.exists() {
                    return Err(format!("Could not download the game: {e}"));
                }
                println!("\nCould not download the update: {e}\nStarting the installed version.");
                std::thread::sleep(Duration::from_secs(3));
            }
        }
        Err(e) if game.exists() => {
            println!("Could not check for updates ({e}). Starting the installed version.");
            std::thread::sleep(Duration::from_secs(2));
        }
        Err(e) => {
            return Err(format!(
                "Could not download the game: {e}\nCheck your internet connection and try again."
            ));
        }
    }

    launch_game(&game, &dir, args)
}

/// Where the game is installed: `%LOCALAPPDATA%\ShitShooter`, or next to the
/// launcher if that variable is somehow missing.
fn install_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(|base| PathBuf::from(base).join("ShitShooter"))
        .or_else(|| Some(std::env::current_exe().ok()?.parent()?.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn download_url(file: &str) -> String {
    format!("https://github.com/{REPO}/releases/latest/download/{file}")
}

fn fetch_text(agent: &ureq::Agent, file: &str) -> Result<String> {
    let text = agent
        .get(download_url(file))
        .call()
        .and_then(|mut response| response.body_mut().read_to_string())
        .map_err(describe)?;
    Ok(text.trim().to_string())
}

/// Downloads a release asset to `dest`, printing progress.
fn download(agent: &ureq::Agent, file: &str, dest: &Path) -> Result<()> {
    let mut response = agent.get(download_url(file)).call().map_err(describe)?;
    let total: Option<u64> = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let mut reader = response.body_mut().as_reader();
    let mut out = fs::File::create(dest).map_err(|e| format!("could not write {}: {e}", dest.display()))?;

    let mut buffer = vec![0; 256 * 1024];
    let mut done: u64 = 0;
    let mut last_print = Instant::now();
    loop {
        let n = reader.read(&mut buffer).map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buffer[..n]).map_err(|e| format!("could not write {}: {e}", dest.display()))?;
        done += n as u64;
        if last_print.elapsed() > Duration::from_millis(100) {
            print_progress(done, total);
            last_print = Instant::now();
        }
    }
    print_progress(done, total);
    println!();
    out.flush().map_err(|e| format!("could not write {}: {e}", dest.display()))?;
    if total.is_some_and(|total| done != total) {
        return Err("the download was cut short".into());
    }
    Ok(())
}

fn print_progress(done: u64, total: Option<u64>) {
    let mb = |bytes: u64| bytes as f64 / 1_048_576.0;
    match total {
        Some(total) if total > 0 => print!(
            "\r  {:.1} / {:.1} MB ({}%)   ",
            mb(done),
            mb(total),
            done * 100 / total
        ),
        _ => print!("\r  {:.1} MB   ", mb(done)),
    }
    let _ = std::io::stdout().flush();
}

/// Downloads the latest game next to the installed one, then swaps it in.
fn install_game(agent: &ureq::Agent, dir: &Path, version: &str) -> Result<()> {
    let partial = dir.join(format!("{GAME_EXE}.download"));
    let result = download(agent, GAME_EXE, &partial);
    if let Err(e) = result {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    let game = dir.join(GAME_EXE);
    // Fails while the game is running, since Windows locks a running exe.
    while let Err(e) = fs::rename(&partial, &game) {
        println!("\nCould not replace the game ({e}).");
        println!("If ShitShooter is still running, close it, then press Enter to try again.");
        pause_quietly();
    }
    // If this fails the next start just downloads the build again.
    let _ = fs::write(dir.join(GAME_VERSION_FILE), version);
    println!("Done.");
    Ok(())
}

fn launch_game(game: &Path, dir: &Path, args: &[String]) -> Result<()> {
    println!("Starting ShitShooter...");
    let mut command = Command::new(game);
    command.args(args).current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Current game builds are GUI apps and get no console from this. Older
        // builds (still installed when offline) had one: they get their own
        // window, as if double-clicked, so this one can close.
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        command.creation_flags(CREATE_NEW_CONSOLE);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start {}: {e}", game.display()))
}

/// Replaces this launcher with the one in the latest release if its version
/// differs, then runs the new one in its place and exits with its exit code.
/// Returns `Ok` when no update is needed.
fn update_launcher(agent: &ureq::Agent, args: &[String]) -> Result<()> {
    // Offline, or a release without the file: stay quiet, the game check reports it.
    let Ok(latest) = fetch_text(agent, LAUNCHER_VERSION_FILE) else {
        return Ok(());
    };
    if latest == VERSION {
        return Ok(());
    }
    println!("Updating the launcher to {latest}...");
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = me.with_extension("exe.new");
    let old = me.with_extension("exe.old");
    if let Err(e) = download(agent, LAUNCHER_EXE, &new) {
        let _ = fs::remove_file(&new);
        return Err(e);
    }
    // A running exe cannot be overwritten, but it can be renamed out of the way.
    let _ = fs::remove_file(&old);
    fs::rename(&me, &old).map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&new, &me) {
        let _ = fs::rename(&old, &me);
        return Err(e.to_string());
    }
    println!();
    let status = Command::new(&me)
        .args(args)
        .env(JUST_UPDATED_ENV, "1")
        .status()
        .map_err(|e| e.to_string())?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Deletes the previous launcher left behind by a self-update. It may still be
/// running for a moment, in which case this is retried on the next start.
fn remove_old_launcher() {
    if let Ok(me) = std::env::current_exe() {
        let _ = fs::remove_file(me.with_extension("exe.old"));
    }
}

fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(404) => "no release found on GitHub yet".into(),
        ureq::Error::StatusCode(code) => format!("GitHub answered with error {code}"),
        other => other.to_string(),
    }
}

fn pause() {
    println!("\nPress Enter to close.");
    pause_quietly();
}

fn pause_quietly() {
    let _ = std::io::stdin().read_line(&mut String::new());
}
