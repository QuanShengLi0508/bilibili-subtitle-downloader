use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Article {
    pub title: String,
    pub body: String,
    pub source: String,
    pub selected: bool,
}

impl Article {
    pub fn from_message(message: &str) -> Result<Self> {
        if message.len() > 2_000_000 {
            bail!("文章过大，请分段获取");
        }
        let value: Value = serde_json::from_str(message)?;
        let source = value["source"].as_str().unwrap_or_default();
        if !crate::external::is_douyin(source) {
            bail!("请在抖音官方文章页面获取正文");
        }
        if value["incomplete"].as_bool() == Some(true) {
            bail!("页面仍有阅读全文或登录提示，请先展开正文再获取");
        }
        let body = value["body"].as_str().unwrap_or_default().trim().to_owned();
        if body.chars().count() < 20 {
            bail!("没有获取到文章正文；图片中的文字暂不支持识别");
        }
        let title = value["title"].as_str().unwrap_or("抖音长文章").trim();
        Ok(Self {
            title: if title.is_empty() {
                "抖音长文章"
            } else {
                title
            }
            .into(),
            body,
            source: source.into(),
            selected: value["selected"].as_bool().unwrap_or(false),
        })
    }

    pub fn export(
        &self,
        directory: &Path,
        format: crate::export::TextFormat,
    ) -> Result<std::path::PathBuf> {
        let path = directory.join(format!(
            "{}_抖音长文章.{}",
            crate::bili::sanitize_filename(&self.title),
            format.extension()
        ));
        crate::export::save(
            &path,
            &self.title,
            &format!("{}\n\n来源：{}", self.body, self.source),
            format,
        )?;
        Ok(path)
    }
}

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
                std::fs::create_dir_all(&profile)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title("拾文 · 抖音长文章（展开正文后点击右下角获取）")
                        .with_inner_size(winit::dpi::LogicalSize::new(1040.0, 700.0)),
                )?;
                let mut context = wry::WebContext::new(Some(profile));
                let proxy = self.proxy.clone();
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_initialization_script(include_str!("douyin_reader.js"))
                    .with_ipc_handler(move |request| {
                        let _ = proxy.send_event(request.body().clone());
                    })
                    .with_url(&self.url)
                    .build(&window)
                    .context("无法打开文章窗口，请检查 WebView2 Runtime")?;
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
            match Article::from_message(&message) {
                Ok(_) => {
                    self.result = Some(Ok(Some(message)));
                    event_loop.exit();
                }
                Err(error) => {
                    rfd::MessageDialog::new()
                        .set_title("抖音长文章")
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
    let url = crate::external::supported_url(url).context("请粘贴抖音文章分享链接")?;
    if !crate::external::is_douyin(&url) {
        bail!("仅支持抖音文章链接");
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
    bail!("文章窗口目前支持 Windows");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_excerpt_and_exports_the_final_paragraph() {
        let mut value = serde_json::json!({"source":"https://www.douyin.com/note/123", "title":"测试长文章",
            "body":format!("{}\n最后一段完整保留", "正文段落。\n".repeat(200)), "incomplete":true});
        assert!(Article::from_message(&value.to_string()).is_err());
        value["incomplete"] = false.into();
        let article = Article::from_message(&value.to_string()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = article
            .export(temp.path(), crate::export::TextFormat::Txt)
            .unwrap();
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("最后一段完整保留"));
        value["source"] = "https://douyin.com.evil.test/note/123".into();
        assert!(Article::from_message(&value.to_string()).is_err());
    }
}
