use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Video {
    pub title: String,
    pub source: String,
    pub media_url: String,
}

fn https_url(input: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(input).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none_or(|port| port == 443))
    .then_some(url)
}

fn media_host(url: &reqwest::Url) -> bool {
    matches!(
        url.host_str(),
        Some("finder.video.qq.com" | "wxapp.tc.qq.com")
    )
}

fn official_page(url: &reqwest::Url) -> bool {
    match url.host_str() {
        Some("channels.weixin.qq.com") => {
            matches!(url.path(), "/" | "/web/pages/feed" | "/web/pages/home")
        }
        Some("weixin.qq.com") => url.path().starts_with("/sph/"),
        _ => false,
    }
}

pub fn link(input: &str) -> Option<String> {
    input
        .split(|ch: char| ch.is_whitespace() || "，。；！【】《》“”".contains(ch))
        .filter_map(|part| {
            let start = part.find("https://")?;
            let candidate =
                part[start..].trim_end_matches([')', ']', '}', ',', '.', ';', '!', '\'', '"']);
            let url = https_url(candidate)?;
            (official_page(&url) || media_host(&url)).then(|| url.to_string())
        })
        .next()
}

impl Video {
    pub fn from_message(message: &str) -> Result<Self> {
        if message.len() > 64_000 {
            bail!("视频信息过大，请重新打开作品");
        }
        let value: Value = serde_json::from_str(message).context("视频信息无法读取")?;
        let source = value["source"].as_str().context("缺少视频号来源页面")?;
        let media_url = value["media_url"]
            .as_str()
            .context("没有获取到可下载的视频地址")?;
        let source_url = https_url(source).context("来源必须是视频号官方 HTTPS 页面")?;
        let media = https_url(media_url)
            .context("当前视频使用临时播放流，暂不能直接下载；可以先导入已有本地视频转写")?;
        if !media_host(&media) {
            bail!("当前视频未提供支持的官方视频直链");
        }
        if !(official_page(&source_url) || (media_host(&source_url) && source_url == media)) {
            bail!("视频来源不是视频号官方页面");
        }
        let title: String = value["title"]
            .as_str()
            .unwrap_or("视频号视频")
            .trim()
            .chars()
            .take(80)
            .collect();
        Ok(Self {
            title: if title.is_empty() {
                "视频号视频".into()
            } else {
                title
            },
            source: source_url.to_string(),
            media_url: media.to_string(),
        })
    }
}

fn valid_ipc_source(message: &str, request_url: &str) -> bool {
    let Some(mut actual) = https_url(request_url).filter(official_page) else {
        return false;
    };
    let Ok(video) = Video::from_message(message) else {
        return false;
    };
    let Some(mut claimed) = https_url(&video.source) else {
        return false;
    };
    actual.set_fragment(None);
    claimed.set_fragment(None);
    actual == claimed
}

fn command(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

fn verify_video(path: &Path) -> Result<()> {
    let probe = command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=format_name,duration:stream=codec_type,codec_name,width,height",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .context("无法验证视频，请重新安装内置视频组件")?;
    let value: Value = serde_json::from_slice(&probe.stdout).unwrap_or_default();
    let container = value["format"]["format_name"].as_str().unwrap_or_default();
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|n| n.parse::<f64>().ok());
    let video = value["streams"].as_array().is_some_and(|streams| {
        streams.iter().any(|stream| {
            stream["codec_type"] == "video"
                && stream["codec_name"]
                    .as_str()
                    .is_some_and(|name| !name.is_empty() && name != "unknown")
                && stream["width"].as_u64().unwrap_or(0) > 0
                && stream["height"].as_u64().unwrap_or(0) > 0
        })
    });
    if !probe.status.success()
        || !container
            .split(',')
            .any(|name| name == "mp4" || name == "mov")
        || !duration.is_some_and(|duration| duration.is_finite() && duration > 0.0)
        || !video
    {
        bail!("当前地址未返回可播放的 MP4 视频，可能已过期或采用受限播放方式；未保存无效文件");
    }
    // A container can be readable while its frames are encrypted or corrupt.
    // Decode the full video before advertising a successful download.
    let decoded = command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-xerror", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-f", "null", "-"])
        .output()
        .context("无法验证视频播放，请重新安装内置视频组件")?;
    if !decoded.status.success() {
        bail!("视频未通过播放检查，可能不完整或采用受限播放方式；未保存无效文件");
    }
    Ok(())
}

pub fn download(video: &Video, output_dir: &Path, progress: &dyn Fn(f64)) -> Result<PathBuf> {
    // Revalidate even when the caller constructed Video directly.
    let video = Video::from_message(
        &serde_json::json!({
            "title": video.title, "source": video.source, "media_url": video.media_url
        })
        .to_string(),
    )?;
    crate::bili::configure_tools_path();
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(3600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("视频重定向过多")
            } else if https_url(attempt.url().as_str()).is_some_and(|url| media_host(&url)) {
                attempt.follow()
            } else {
                attempt.error("视频跳转到了非支持的服务器")
            }
        }))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36")
        .build()?;
    let mut response = client
        .get(&video.media_url)
        .header("Referer", "https://channels.weixin.qq.com/")
        .send()
        .context("视频连接失败，请重新打开官方页面并读取")?
        .error_for_status()
        .context("视频地址已失效或需要访问权限，请重新打开官方页面")?;
    let total = response.content_length();
    const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
    if total.is_some_and(|length| length > MAX_BYTES) {
        bail!("视频超过 4 GB，暂不支持直接保存");
    }
    std::fs::create_dir_all(output_dir)?;
    let mut file = tempfile::Builder::new()
        .prefix(".shiwen-video-")
        .suffix(".mp4")
        .tempfile_in(output_dir)?;
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let length = response
            .read(&mut buffer)
            .context("视频下载中断，请重新获取")?;
        if length == 0 {
            break;
        }
        downloaded += length as u64;
        if downloaded > MAX_BYTES {
            bail!("视频超过 4 GB，已停止下载");
        }
        file.write_all(&buffer[..length])?;
        if let Some(total) = total.filter(|&n| n > 0) {
            progress((downloaded as f64 / total as f64).min(1.0) * 0.9);
        }
    }
    if downloaded == 0 || total.is_some_and(|total| total != downloaded) {
        bail!("视频没有完整下载，未保存无效文件");
    }
    file.flush()?;
    file.as_file().sync_all()?;
    progress(0.92);
    verify_video(file.path())?;
    let name = crate::bili::sanitize_filename(&video.title);
    // Avoid replacing an earlier download, including a file created meanwhile.
    let mut file = file;
    for index in 0..10_000 {
        let name = if index == 0 {
            format!("{name}.mp4")
        } else {
            format!("{name} ({index}).mp4")
        };
        let target = output_dir.join(name);
        match file.persist_noclobber(&target) {
            Ok(_) => {
                progress(1.0);
                return Ok(target);
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                file = error.file
            }
            Err(error) => return Err(error.error).context("视频已下载，但保存失败"),
        }
    }
    bail!("同名文件过多，请更换输出目录")
}

#[cfg(windows)]
pub fn open_reader(input: &str, output: &Path) -> Result<bool> {
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop},
        window::{Window, WindowId},
    };
    let url = link(input).context("请粘贴视频号官方网页链接；微信聊天中的视频卡片不能直接解析")?;
    if media_host(&reqwest::Url::parse(&url)?) {
        std::fs::write(
            output,
            serde_json::json!({"title":"视频号视频", "source":url, "media_url":url}).to_string(),
        )?;
        return Ok(true);
    }
    struct Reader {
        webview: Option<wry::WebView>,
        window: Option<Window>,
        url: String,
        proxy: winit::event_loop::EventLoopProxy<String>,
        result: Option<Result<String>>,
    }
    impl ApplicationHandler<String> for Reader {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let setup = (|| -> Result<_> {
                let profile =
                    crate::zhihu_login::session_directory().join("wechat-channels-webview");
                std::fs::create_dir_all(&profile)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title("拾文 · 视频号（播放后点击「读取当前视频」）")
                        .with_inner_size(winit::dpi::LogicalSize::new(1040.0, 760.0)),
                )?;
                let mut context = wry::WebContext::new(Some(profile));
                let proxy = self.proxy.clone();
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_initialization_script(include_str!("wechat_channels_reader.js"))
                    .with_ipc_handler(move |request| {
                        if valid_ipc_source(request.body(), &request.uri().to_string()) {
                            let _ = proxy.send_event(request.body().clone());
                        }
                    })
                    .with_navigation_handler(|url| {
                        https_url(&url).is_some_and(|url| {
                            matches!(
                                url.host_str(),
                                Some(
                                    "channels.weixin.qq.com"
                                        | "weixin.qq.com"
                                        | "open.weixin.qq.com"
                                        | "support.weixin.qq.com"
                                )
                            )
                        })
                    })
                    .with_url(&self.url)
                    .build(&window)
                    .context("无法打开视频号窗口，请检查 WebView2 Runtime")?;
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
            match Video::from_message(&message) {
                Ok(_) => {
                    self.result = Some(Ok(message));
                    event_loop.exit();
                }
                Err(error) => {
                    rfd::MessageDialog::new()
                        .set_title("视频号读取")
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
    let event_loop = EventLoop::<String>::with_user_event().build()?;
    let mut reader = Reader {
        webview: None,
        window: None,
        url,
        proxy: event_loop.create_proxy(),
        result: None,
    };
    event_loop.run_app(&mut reader)?;
    if let Some(message) = reader.result {
        std::fs::write(output, message?)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(not(windows))]
pub fn open_reader(_: &str, _: &Path) -> Result<bool> {
    bail!("视频号网页读取目前支持 Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_official_https_share_pages_and_media() {
        assert!(
            link("视频分享 https://channels.weixin.qq.com/web/pages/feed?oid=abc&nid=def")
                .is_some()
        );
        assert!(link("https://weixin.qq.com/sph/Abc123").is_some());
        for invalid in [
            "https://channels.weixin.qq.com.evil.test/web/pages/feed?oid=abc",
            "https://evil.test/?url=https://channels.weixin.qq.com/web/pages/feed?oid=abc",
            "https://user:pass@channels.weixin.qq.com/web/pages/feed",
            "https://channels.weixin.qq.com:8080/web/pages/feed",
            "https://weixin.qq.com/unknown",
            "#视频号：某视频",
        ] {
            assert!(link(invalid).is_none(), "{invalid}");
        }
    }
    #[test]
    fn rejects_blob_external_sources_and_fake_cdn_hosts() {
        let mut value = serde_json::json!({"source":"https://channels.weixin.qq.com/web/pages/feed?oid=a", "title":"作品", "media_url":"https://finder.video.qq.com/video.mp4?token=original"});
        assert_eq!(
            Video::from_message(&value.to_string()).unwrap().title,
            "作品"
        );
        for invalid in [
            "blob:https://channels.weixin.qq.com/uuid",
            "https://finder.video.qq.com.evil.test/video.mp4",
            "file:///C:/video.mp4",
            "http://finder.video.qq.com/video.mp4",
        ] {
            value["media_url"] = invalid.into();
            assert!(Video::from_message(&value.to_string()).is_err());
        }
        value["media_url"] = "https://finder.video.qq.com/video.mp4".into();
        value["source"] = "https://evil.test/web/pages/feed".into();
        assert!(Video::from_message(&value.to_string()).is_err());
    }
    #[test]
    fn ipc_checks_real_sender_and_current_work() {
        let message = serde_json::json!({"source":"https://channels.weixin.qq.com/web/pages/feed?oid=a#part", "media_url":"https://finder.video.qq.com/video.mp4"}).to_string();
        assert!(valid_ipc_source(
            &message,
            "https://channels.weixin.qq.com/web/pages/feed?oid=a"
        ));
        assert!(!valid_ipc_source(
            &message,
            "https://channels.weixin.qq.com/web/pages/feed?oid=b"
        ));
        assert!(!valid_ipc_source(&message, "https://open.weixin.qq.com/"));
        assert!(!valid_ipc_source(&message, "https://evil.test/"));
    }
    #[test]
    fn playable_video_validation_rejects_invalid_files() {
        crate::bili::configure_tools_path();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.mp4");
        std::fs::write(&path, "<html>Login required</html>").unwrap();
        assert!(verify_video(&path).is_err());
    }
    #[test]
    #[ignore = "需要内置 ffmpeg 和 ffprobe，验证真实 MP4 解码"]
    fn native_validation_accepts_video_and_rejects_audio_only() {
        crate::bili::configure_tools_path();
        let temp = tempfile::tempdir().unwrap();
        let video = temp.path().join("video.mp4");
        let result = command("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:r=5",
                "-t",
                "1",
                "-c:v",
                "mpeg4",
            ])
            .arg(&video)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        verify_video(&video).unwrap();
        let audio = temp.path().join("audio.mp4");
        let result = command("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=16000",
                "-t",
                "1",
                "-c:a",
                "aac",
            ])
            .arg(&audio)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(verify_video(&audio).is_err());
    }
}
