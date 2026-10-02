#[path = "../bili.rs"]
#[allow(dead_code)]
mod bili;
#[path = "../cli.rs"]
mod cli;

fn main() {
    bili::configure_tools_path();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法: bili-subtitle-cli.exe [--srt|--streams|--video] <链接> [清晰度ID]");
        std::process::exit(2);
    }
    if let Err(error) = cli::run(&args.join(" ")) {
        eprintln!("错误: {error:#}");
        std::process::exit(1);
    }
}
