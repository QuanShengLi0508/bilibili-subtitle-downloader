use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Accept only public article routes on the actual Official Accounts host.
fn parse_article_url(input: &str) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(input).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str() != Some("mp.weixin.qq.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let path = url.path();
    let short = path.strip_prefix("/s/").is_some_and(|id| {
        !id.is_empty()
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    });
    let query_article = path == "/s"
        && ["__biz", "mid", "idx", "sn"].iter().all(|key| {
            url.query_pairs()
                .any(|(name, value)| name == *key && !value.is_empty())
        });
    if !short && !query_article {
        return None;
    }
    url.set_scheme("https").ok()?;
    url.set_fragment(None);
    Some(url)
}

/// A copied share message can contain Chinese punctuation immediately before its link.
pub fn article_url(input: &str) -> Option<String> {
    for (start, _) in input.match_indices("http") {
        let rest = &input[start..];
        let candidate = rest
            .split(|c: char| c.is_whitespace() || "<>\"'，。；！、【】（）「」《》".contains(c))
            .next()?
            .trim_end_matches([')', ']', '}', ',', ';', '!', '.']);
        if let Some(url) = parse_article_url(&candidate.replace("&amp;", "&")) {
            return Some(url.to_string());
        }
    }
    None
}

fn image_url(input: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(input).ok()?;
    let host = url.host_str()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && (host == "qpic.cn" || host.ends_with(".qpic.cn") || host == "mmbiz.qlogo.cn"))
        .then_some(url)
}

fn valid_ipc_source(message: &str, request_url: &str) -> bool {
    if message.len() > 4_000_000 {
        return false;
    }
    let Some(actual) = parse_article_url(request_url) else {
        return false;
    };
    if !request_url.starts_with("https://") {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(message) else {
        return false;
    };
    let Some(claimed) = parse_article_url(value["source"].as_str().unwrap_or_default()) else {
        return false;
    };
    actual == claimed
}

#[derive(Clone, Debug)]
pub struct Article {
    pub title: String,
    pub body: String,
    pub source: String,
    pub selected: bool,
    pub images: Vec<String>,
    pub author: String,
    pub published_at: String,
}

impl Article {
    pub fn from_message(message: &str) -> Result<Self> {
        if message.len() > 4_000_000 {
            bail!("文章数据过大，未保存，请减少选取内容");
        }
        let value: Value = serde_json::from_str(message).context("公众号返回的数据无效")?;
        if value["kind"] != "wechat-article" {
            bail!("不是公众号文章数据");
        }
        let source = parse_article_url(value["source"].as_str().unwrap_or_default())
            .context("请在 mp.weixin.qq.com 官方文章页面获取正文")?
            .to_string();
        if value["complete"].as_bool() != Some(true) || value["incomplete"].as_bool() == Some(true)
        {
            bail!("正文尚未完整显示，请先在文章窗口完成验证、登录或阅读全文");
        }
        let body = value["body"].as_str().unwrap_or_default().trim().to_owned();
        let title = value["title"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if title.is_empty() || title.chars().count() > 500 {
            bail!("未获取到有效文章标题，请等待文章加载后重试");
        }
        let entries = value["images"].as_array().context("文章配图数据缺失")?;
        if entries.len() > 300 {
            bail!("文章配图超过 300 张，本次未保存");
        }
        let mut images = Vec::new();
        for entry in entries {
            let url = image_url(entry.as_str().context("文章图片链接无效")?)
                .context("文章包含非微信官方图片来源，本次未保存")?
                .to_string();
            if !images.contains(&url) {
                images.push(url);
            }
        }
        if body.is_empty() && images.is_empty() {
            bail!("未获取到文章正文或配图，请等待加载完成后重试");
        }
        Ok(Self {
            title,
            body,
            source,
            selected: value["selected"].as_bool().unwrap_or(false),
            images,
            author: value["author"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned(),
            published_at: value["published_at"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned(),
        })
    }

    fn document_body(&self, body: &str) -> String {
        let mut metadata = Vec::new();
        if !self.author.is_empty() {
            metadata.push(format!("作者：{}", self.author));
        }
        if !self.published_at.is_empty() {
            metadata.push(format!("发布时间：{}", self.published_at));
        }
        metadata.push(format!("来源：{}", self.source));
        if self.selected {
            metadata.push("内容范围：手动选取的文字".into());
        }
        format!("{}\n\n{}", metadata.join("\n"), body)
    }

    pub fn export(&self, directory: &Path, format: crate::export::TextFormat) -> Result<PathBuf> {
        std::fs::create_dir_all(directory)?;
        let name = crate::bili::sanitize_filename(&self.title);
        if self.images.is_empty() {
            let path = directory.join(format!("{name}_公众号.{}", format.extension()));
            crate::export::save(&path, &self.title, &self.document_body(&self.body), format)?;
            return Ok(path);
        }
        // Keep the package private until every image and the document have succeeded.
        let folder = tempfile::Builder::new()
            .prefix(&format!("{name}_公众号_"))
            .tempdir_in(directory)?;
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(45))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 5 && image_url(attempt.url().as_str()).is_some() {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()?;
        let mut body = self.body.clone();
        for (index, url) in self.images.iter().enumerate() {
            image_url(url).context("文章配图链接无效")?;
            let response = client
                .get(url)
                .header("Referer", "https://mp.weixin.qq.com/")
                .send()?
                .error_for_status()?;
            let mut bytes = Vec::new();
            response.take(50_000_001).read_to_end(&mut bytes)?;
            if bytes.len() > 50_000_000 {
                bail!("第 {} 张配图超过 50 MB，本次未保存", index + 1);
            }
            let extension = image_extension(&bytes)
                .with_context(|| format!("第 {} 张配图未能下载为图片，本次未保存", index + 1))?;
            let filename = format!("配图{:03}.{extension}", index + 1);
            std::fs::File::create(folder.path().join(&filename))?.write_all(&bytes)?;
            // Markdown links remain useful offline beside the downloaded images.
            body = body.replace(&format!("]({url})"), &format!("]({filename})"));
        }
        crate::export::save(
            &folder.path().join(format!("{name}.{}", format.extension())),
            &self.title,
            &self.document_body(&body),
            format,
        )?;
        Ok(folder.keep())
    }
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if bytes.get(4..12) == Some(b"ftypavif") || bytes.get(4..12) == Some(b"ftypavis") {
        Some("avif")
    } else {
        None
    }
}

#[cfg(windows)]
pub fn open_reader(input: &str, output: &Path) -> Result<bool> {
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
                let profile =
                    crate::zhihu_login::session_directory().join("wechat-article-webview");
                std::fs::create_dir_all(&profile)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title("拾文 · 微信公众号（正文显示完整后点击获取）")
                        .with_inner_size(winit::dpi::LogicalSize::new(1040.0, 720.0)),
                )?;
                let mut context = wry::WebContext::new(Some(profile));
                let proxy = self.proxy.clone();
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_initialization_script(include_str!("wechat_article_reader.js"))
                    .with_navigation_handler(|input| {
                        reqwest::Url::parse(&input).is_ok_and(|url| {
                            url.scheme() == "https"
                                && url.host_str() == Some("mp.weixin.qq.com")
                                && url.username().is_empty()
                                && url.password().is_none()
                                && url.port().is_none()
                        })
                    })
                    .with_ipc_handler(move |request| {
                        if valid_ipc_source(request.body(), &request.uri().to_string()) {
                            let _ = proxy.send_event(request.body().clone());
                        }
                    })
                    .with_url(&self.url)
                    .build(&window)
                    .context("无法打开公众号窗口，请检查 WebView2 Runtime")?;
                Ok((window, webview))
            })();
            match setup {
                Ok((window, webview)) => {
                    self.window = Some(window);
                    self.webview = Some(webview);
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
                        .set_title("微信公众号")
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
    let url = article_url(input).context("请粘贴微信公众号文章链接或包含链接的分享文字")?;
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
    bail!("公众号文章窗口目前支持 Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload() -> Value {
        serde_json::json!({"kind":"wechat-article", "source":"https://mp.weixin.qq.com/s/Example_123-abc", "title":"公众号测试", "body":"第一段\n\n最后一段完整保留。", "complete":true, "author":"文章作者", "published_at":"2026-10-07", "images":[]})
    }
    #[test]
    fn validates_shared_article_urls_without_accepting_lookalikes() {
        assert_eq!(
            article_url("文章：https://mp.weixin.qq.com/s/Test_123-Abc。"),
            Some("https://mp.weixin.qq.com/s/Test_123-Abc".into())
        );
        assert!(
            article_url("https://mp.weixin.qq.com/s?__biz=Abc==&amp;mid=123&idx=1&sn=abc#rd")
                .is_some()
        );
        for url in [
            "https://mp.weixin.qq.com.evil.test/s/test",
            "https://evil.test/?url=https%3A%2F%2Fmp.weixin.qq.com%2Fs%2Ftest",
            "https://mp.weixin.qq.com@evil.test/s/test",
            "https://user@mp.weixin.qq.com/s/test",
            "https://mp.weixin.qq.com:8443/s/test",
            "https://mp.weixin.qq.com/s",
            "https://mp.weixin.qq.com/cgi-bin/home",
            "file://mp.weixin.qq.com/s/test",
        ] {
            assert!(article_url(url).is_none(), "{url}");
        }
    }
    #[test]
    fn rejects_incomplete_spoofed_and_invalid_image_messages() {
        let mut value = payload();
        value["complete"] = false.into();
        assert!(Article::from_message(&value.to_string()).is_err());
        value["complete"] = true.into();
        value["source"] = "https://mp.weixin.qq.com.evil.test/s/test".into();
        assert!(Article::from_message(&value.to_string()).is_err());
        value = payload();
        value["images"] = serde_json::json!(["https://mmbiz.qpic.cn.evil.test/pic.jpg"]);
        assert!(Article::from_message(&value.to_string()).is_err());
        value["images"] = serde_json::json!([
            "https://mmbiz.qpic.cn/pic.jpg",
            "https://mmbiz.qpic.cn/pic.jpg"
        ]);
        assert_eq!(
            Article::from_message(&value.to_string())
                .unwrap()
                .images
                .len(),
            1
        );
        assert!(image_extension(b"<html>verification</html>").is_none());
    }
    #[test]
    fn ipc_checks_actual_article_url_not_just_claimed_official_host() {
        let message = payload().to_string();
        assert!(valid_ipc_source(
            &message,
            "https://mp.weixin.qq.com/s/Example_123-abc#rd"
        ));
        for actual in [
            "https://evil.test/s/Example_123-abc",
            "https://mp.weixin.qq.com/cgi-bin/login",
            "https://mp.weixin.qq.com/s/AnotherArticle",
            "http://mp.weixin.qq.com/s/Example_123-abc",
        ] {
            assert!(!valid_ipc_source(&message, actual), "{actual}");
        }
    }
    #[test]
    fn preserves_last_paragraph_metadata_and_short_real_articles() {
        let article = Article::from_message(&payload().to_string()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = article
            .export(directory.path(), crate::export::TextFormat::Markdown)
            .unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        for expected in [
            "最后一段完整保留",
            "文章作者",
            "2026-10-07",
            "https://mp.weixin.qq.com/s/Example_123-abc",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        let mut short = payload();
        short["body"] = "简短通知。".into();
        assert!(Article::from_message(&short.to_string()).is_ok());
    }
}
