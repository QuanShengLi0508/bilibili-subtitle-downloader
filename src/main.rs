mod bili;
mod cli;
mod external;
mod gui;
mod transcribe;
mod webtext;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        // 命令行模式: bili-subtitle-downloader.exe <链接>
        if let Err(e) = cli::run(&args[1]) {
            eprintln!("错误: {e:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
    gui::run()
}
