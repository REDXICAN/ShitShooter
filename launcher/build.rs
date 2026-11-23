// Embeds the game's icon.ico (in the repo root) into the launcher exe.
fn main() {
    println!("cargo:rerun-if-changed=../icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../icon.ico");
        res.compile().expect("could not embed icon.ico");
    }
}
