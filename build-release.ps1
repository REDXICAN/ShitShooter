# Builds the exe to send to friends: dist\shitshooter.exe
#
# Compared to a plain `cargo build --release`, the C runtime is linked into the
# exe, so it runs on PCs that do not have the Visual C++ Redistributable.
# It builds in its own folder, so it works while the game is running.
$ErrorActionPreference = 'Stop'
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --target-dir target\dist
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force dist | Out-Null
try {
    Copy-Item target\dist\release\shitshooter.exe dist\shitshooter.exe -Force
} catch {
    Write-Host "dist\shitshooter.exe is in use, so it was not replaced. Close the game and run this again."
    exit 1
}
Write-Host "Built dist\shitshooter.exe"
