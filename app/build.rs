//! Puts the logo into the Windows program, so Explorer and the taskbar show it.
//! Other systems take the icon from their package (see packaging/).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=The program's icon could not be added: {e}");
        }
    }
}
