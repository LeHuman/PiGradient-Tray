fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "PiGradient Tray");
        res.set("FileDescription", env!("CARGO_PKG_DESCRIPTION"));
        if let Err(err) = res.compile() {
            println!("cargo:warning=winresource skipped: {err}");
        }
    }
}
