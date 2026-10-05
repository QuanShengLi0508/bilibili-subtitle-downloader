#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod bili;
#[allow(dead_code)]
mod cli;
mod export;
mod external;
mod gui;
mod transcribe;
mod zhihu;
mod zhihu_login;

fn main() -> eframe::Result<()> {
    let douyin_login = std::env::args().any(|arg| arg == "--douyin-login");
    if douyin_login || std::env::args().any(|arg| arg == "--zhihu-login") {
        let code = match if douyin_login {
            zhihu_login::run_douyin_window()
        } else {
            zhihu_login::run_window()
        } {
            Ok(true) => 0,
            Ok(false) => 2,
            Err(error) => {
                rfd::MessageDialog::new()
                    .set_title(if douyin_login {
                        "抖音登录"
                    } else {
                        "知乎登录"
                    })
                    .set_description(format!("{error:#}"))
                    .set_level(rfd::MessageLevel::Error)
                    .show();
                1
            }
        };
        std::process::exit(code);
    }
    bili::configure_tools_path();
    gui::run()
}
