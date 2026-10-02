fn main() {
    println!("cargo:rerun-if-changed=assets/subtitle-extractor.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("assets/subtitle-extractor.ico")
            .set("ProductName", "拾文")
            .set("FileDescription", "拾文：字幕提取、视频下载与音视频转文字")
            .compile()
            .expect("Failed to embed the application icon");
    }
}
