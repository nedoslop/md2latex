fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "MarkDown to LaTeX");
        res.set("FileDescription", "Writing documents made easier");
        res.set("LegalCopyright", "Copyright © 2026 Pixelsuft");
        // res.set_icon("test.ico");
        res.set_manifest_file("settings.manifest");
        res.compile().unwrap();
    }
}
