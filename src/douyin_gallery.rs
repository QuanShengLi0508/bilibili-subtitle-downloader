use anyhow::{bail, Context, Result};
use std::path::Path;

#[cfg(windows)]
pub fn open_reader(url: &str, output: &Path) -> Result<bool> {
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop},
        window::{Window, WindowId},
    };
    struct Reader {
        webview: Option<wry::WebView>,
        window: Option<Window>,
        url: String,
        proxy: winit::event_loop::EventLoopProxy<String>,
        result: Option<Result<Option<String>>>,
    }
    impl ApplicationHandler<String> for Reader {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let setup = (|| -> Result<_> {
                let profile = crate::zhihu_login::session_directory().join("douyin-webview");
                let xhs = crate::external::is_xhs(&self.url);
                let profile = if xhs {
                    crate::zhihu_login::session_directory().join("xhs-webview")
                } else {
                    profile
                };
                std::fs::create_dir_all(&profile)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title(
                            if xhs && self.url == "https://www.xiaohongshu.com/explore" {
                                "拾文 · 小红书登录（完成后关闭窗口）"
                            } else if xhs {
                                "拾文 · 读取小红书图文（完成后自动返回）"
                            } else {
                                "拾文 · 读取抖音图文（完成后自动返回）"
                            },
                        )
                        .with_inner_size(winit::dpi::LogicalSize::new(1040.0, 700.0)),
                )?;
                let mut context = wry::WebContext::new(Some(profile));
                let proxy = self.proxy.clone();
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_initialization_script(if xhs {
                        include_str!("xhs_gallery.js")
                    } else {
                        include_str!("douyin_gallery.js")
                    })
                    .with_ipc_handler(move |request| {
                        let _ = proxy.send_event(request.body().clone());
                    })
                    .with_url(&self.url)
                    .build(&window)
                    .context("无法打开图文窗口，请检查 WebView2 Runtime")?;
                Ok((window, webview))
            })();
            match setup {
                Ok((window, webview)) => {
                    self.webview = Some(webview);
                    self.window = Some(window);
                }
                Err(error) => {
                    self.result = Some(Err(error));
                    event_loop.exit();
                }
            }
        }
        fn user_event(&mut self, event_loop: &ActiveEventLoop, message: String) {
            match crate::external::Gallery::from_message(&message, &self.url) {
                Ok(_) => {
                    self.result = Some(Ok(Some(message)));
                    event_loop.exit();
                }
                Err(error) => {
                    rfd::MessageDialog::new()
                        .set_title("图文提取")
                        .set_description(error.to_string())
                        .show();
                }
            }
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if matches!(event, WindowEvent::CloseRequested) {
                event_loop.exit();
            }
        }
    }
    let url = crate::external::supported_url(url).context("请粘贴图文分享链接")?;
    if !crate::external::is_douyin(&url) && !crate::external::is_xhs(&url) {
        bail!("仅支持抖音或小红书图文链接");
    }
    let event_loop = EventLoop::<String>::with_user_event().build()?;
    let mut reader = Reader {
        webview: None,
        window: None,
        url,
        proxy: event_loop.create_proxy(),
        result: None,
    };
    event_loop.run_app(&mut reader)?;
    match reader.result.unwrap_or(Ok(None))? {
        Some(message) => {
            std::fs::write(output, message)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(not(windows))]
pub fn open_reader(_: &str, _: &Path) -> Result<bool> {
    bail!("图文窗口目前支持 Windows");
}
