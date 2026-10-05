use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Article {
    pub title: String,
    pub body: String,
    pub source: String,
    pub selected: bool,
    pub images: Vec<String>,
}

pub fn bili_url(input: &str) -> Option<String> {
    input.split_whitespace().find_map(|part| {
        let url = reqwest::Url::parse(part).ok()?;
        let host = url.host_str()?;
        if !(host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv") {
            return None;
        }
        if host == "b23.tv"
            || url.path().starts_with("/opus/")
            || url.path().starts_with("/read/cv")
            || url.path().starts_with("/dynamic/")
            || host == "t.bilibili.com"
        {
            Some(url.to_string())
        } else {
            None
        }
    })
}

impl Article {
    pub fn from_message(message: &str) -> Result<Self> {
        if message.len() > 2_000_000 {
            bail!("文章过大，请分段获取");
        }
        let value: Value = serde_json::from_str(message)?;
        let source = value["source"].as_str().unwrap_or_default();
        if !crate::external::is_douyin(source) && bili_url(source).is_none() {
            bail!("请在对应平台的官方图文或文章页面获取正文");
        }
        if value["incomplete"].as_bool() == Some(true) {
            bail!("页面仍有阅读全文或登录提示，请先展开正文再获取");
        }
        let body = value["body"].as_str().unwrap_or_default().trim().to_owned();
        if body.chars().count() < 20 {
            bail!("没有获取到文章正文；图片中的文字暂不支持识别");
        }
        let title = value["title"].as_str().unwrap_or("抖音长文章").trim();
        let images = value["images"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter(|link| {
                        reqwest::Url::parse(link).ok().is_some_and(|u| {
                            u.scheme() == "https"
                                && u.host_str()
                                    .is_some_and(|h| h == "hdslb.com" || h.ends_with(".hdslb.com"))
                        })
                    })
                    .map(str::to_owned)
                    .take(200)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            images,
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
        if !self.images.is_empty() && bili_url(&self.source).is_some() {
            return crate::external::download_gallery(
                &crate::external::ExternalVideo {
                    title: self.title.clone(),
                    url: self.source.clone(),
                    gallery: Some(crate::external::Gallery {
                        description: self.body.clone(),
                        author: String::new(),
                        images: self.images.clone(),
                    }),
                },
                directory,
                format,
                &|_| {},
            );
        }
        let path = directory.join(format!(
            "{}_{}.{}",
            crate::bili::sanitize_filename(&self.title),
            if bili_url(&self.source).is_some() {
                "B站图文"
            } else {
                "抖音长文章"
            },
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
                let bili = bili_url(&self.url).is_some();
                let profile = crate::zhihu_login::session_directory().join(if bili {
                    "bili-webview"
                } else {
                    "douyin-webview"
                });
                std::fs::create_dir_all(&profile)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title("拾文 · 图文 / 文章（展开正文后点击右下角获取）")
                        .with_inner_size(winit::dpi::LogicalSize::new(1040.0, 700.0)),
                )?;
                let mut context = wry::WebContext::new(Some(profile));
                let proxy = self.proxy.clone();
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_initialization_script(if bili {
                        include_str!("bili_reader.js")
                    } else {
                        include_str!("douyin_reader.js")
                    })
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
    let url = bili_url(url)
        .or_else(|| crate::external::supported_url(url))
        .context("请粘贴文章分享链接")?;
    if !crate::external::is_douyin(&url) && bili_url(&url).is_none() {
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
    fn bili_article_accepts_official_sources_and_rejects_lookalikes() {
        assert!(bili_url("https://www.bilibili.com/opus/123").is_some());
        assert!(bili_url("https://www.bilibili.com/read/cv123").is_some());
        assert!(bili_url("https://bilibili.com.evil.test/opus/123").is_none());
        assert!(bili_url("https://www.bilibili.com/video/BV123").is_none());
        let value = serde_json::json!({"source":"https://www.bilibili.com/opus/123", "title":"图文测试", "body":"首段正文。".repeat(20), "images":["https://i0.hdslb.com/bfs/article/test.jpg","https://evil.test/test.jpg"]});
        let article = Article::from_message(&value.to_string()).unwrap();
        assert_eq!(article.images.len(), 1);
        assert_eq!(article.title, "图文测试");
    }
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
