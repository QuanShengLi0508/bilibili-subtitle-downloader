#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod bili;
#[allow(dead_code)]
mod cli;
mod comments;
mod douyin_article;
mod douyin_gallery;
mod export;
mod external;
mod gui;
mod transcribe;
mod wechat_article;
mod wechat_channels;
mod youtube;
mod zhihu;
mod zhihu_login;

fn main() -> eframe::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if let Some(index) = args
        .iter()
        .position(|arg| matches!(arg.as_str(), "--wechat-article" | "--wechat-channels"))
    {
        let result = (|| -> anyhow::Result<bool> {
            let input = args
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("缺少微信内容链接"))?;
            let output = args
                .get(index + 2)
                .ok_or_else(|| anyhow::anyhow!("缺少预览输出路径"))?;
            if args[index] == "--wechat-article" {
                wechat_article::open_reader(input, std::path::Path::new(output))
            } else {
                wechat_channels::open_reader(input, std::path::Path::new(output))
            }
        })();
        std::process::exit(match result {
            Ok(true) => 0,
            Ok(false) => 2,
            Err(error) => {
                rfd::MessageDialog::new()
                    .set_title("微信内容获取")
                    .set_description(format!("{error:#}"))
                    .show();
                1
            }
        });
    }
    if let Some(index) = args.iter().position(|arg| arg == "--comments-reader") {
        let result = (|| -> anyhow::Result<bool> {
            let input = args
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("缺少评论链接"))?;
            let output = args
                .get(index + 2)
                .ok_or_else(|| anyhow::anyhow!("缺少评论预览路径"))?;
            comments::open_reader(input, std::path::Path::new(output))
        })();
        let code = match result {
            Ok(true) => 0,
            Ok(false) => 2,
            Err(error) => {
                rfd::MessageDialog::new()
                    .set_title("检测评论")
                    .set_description(format!("{error:#}"))
                    .show();
                1
            }
        };
        std::process::exit(code);
    }
    if let Some(index) = args
        .iter()
        .position(|arg| arg == "--douyin-gallery" || arg == "--xhs-gallery")
    {
        let result = (|| -> anyhow::Result<bool> {
            let url = args
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("缺少图文链接"))?;
            let output = args
                .get(index + 2)
                .ok_or_else(|| anyhow::anyhow!("缺少图文输出路径"))?;
            douyin_gallery::open_reader(url, std::path::Path::new(output))
        })();
        let code = match result {
            Ok(true) => 0,
            Ok(false) => 2,
            Err(error) => {
                rfd::MessageDialog::new()
                    .set_title("抖音图文")
                    .set_description(format!("{error:#}"))
                    .show();
                1
            }
        };
        std::process::exit(code);
    }
    if let Some(index) = args
        .iter()
        .position(|arg| arg == "--douyin-article" || arg == "--bili-article")
    {
        let result = (|| -> anyhow::Result<bool> {
            let url = args
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("缺少文章链接"))?;
            let output = args
                .get(index + 2)
                .ok_or_else(|| anyhow::anyhow!("缺少预览输出路径"))?;
            douyin_article::open_reader(url, std::path::Path::new(output))
        })();
        let code = match result {
            Ok(true) => 0,
            Ok(false) => 2,
            Err(error) => {
                rfd::MessageDialog::new()
                    .set_title("抖音长文章")
                    .set_description(format!("{error:#}"))
                    .show();
                1
            }
        };
        std::process::exit(code);
    }
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
