// Embeds icon.ico into shitshooter.exe, so Explorer, shortcuts and the taskbar
// show it. The game also loads it back by this ID to set the window's icon.
fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id("icon.ico", "1");
        res.compile().expect("could not embed icon.ico");
    }
}
