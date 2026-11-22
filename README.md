# ShitShooter

A fast free-for-all arena shooter with self-hosted lobbies.

## Playing

1. Download **ShitShooterLauncher.exe** from the [latest release](https://github.com/badubies/ShitShooter/releases/latest).
2. Right-click it → **Properties** → tick **Unblock** → **OK**. (Windows blocks downloaded exes that aren't signed. Without this, double-clicking it does nothing.)
3. Run it. It downloads the game, and on every start after that it checks for a new build and installs it before starting the game.

The game is installed to `%LOCALAPPDATA%\ShitShooter`, so the launcher can live anywhere (desktop, downloads...). Without internet it starts the version already installed. Any arguments given to the launcher are passed on to the game, e.g. `ShitShooterLauncher.exe --join 1.2.3.4`.

## Releases

Every push to `main` is built by GitHub Actions ([.github/workflows/release.yml](.github/workflows/release.yml)) and published as a release named `build-<number>`. The last 10 are kept.

To ship a new launcher, bump `version` in [launcher/Cargo.toml](launcher/Cargo.toml). Launchers already installed see the new version in the next release and replace themselves.

## Building locally

- `cargo run`: run the game.
- `.\build-release.ps1`: build `dist\shitshooter.exe` with the C runtime linked in.
- `cargo build --release -p shitshooter-launcher`: build the launcher.
