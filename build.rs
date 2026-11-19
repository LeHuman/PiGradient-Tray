fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "PiGradient Tray");
        // Task Manager's "Name" column shows FileDescription: keep it short and
        // human, not the long crate description.
        res.set("FileDescription", "PiGradient Tray");
        res.set("Comments", env!("CARGO_PKG_DESCRIPTION"));
        if let Err(err) = res.compile() {
            println!("cargo:warning=winresource skipped: {err}");
        }
    }
}
