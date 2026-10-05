use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Clone, Debug)]
pub struct ExternalVideo {
    pub title: String,
    pub url: String,
    pub gallery: Option<Gallery>,
}

#[derive(Clone, Debug)]
pub struct Gallery {
    pub description: String,
    pub author: String,
    pub images: Vec<String>,
}

impl Gallery {
    pub fn from_message(message: &str, expected_url: &str) -> Result<Self> {
        if message.len() > 2_000_000 {
            bail!("图文数据过大");
        }
        let value: Value = serde_json::from_str(message)?;
        let source = value["source"].as_str().unwrap_or_default();
        let xhs = is_xhs(expected_url);
        let matches = if xhs {
            is_xhs(source) && xhs_id(source).is_some() && xhs_id(source) == xhs_id(expected_url)
        } else {
            is_douyin(source)
                && note_id(source).is_some()
                && note_id(source) == note_id(expected_url)
        };
        if !matches {
            bail!("当前页面不是所选的图文作品");
        }
        let entries = value["images"].as_array().context("没有读取到作品图片")?;
        if entries.is_empty() || entries.len() > 200 {
            bail!("未获取完整图片，请等待作品加载");
        }
        let mut images = Vec::new();
        for entry in entries {
            let url = entry.as_str().context("图片链接无效")?;
            let parsed = reqwest::Url::parse(url)?;
            let host = parsed.host_str().unwrap_or_default();
            let allowed = if xhs {
                host == "xhscdn.com" || host.ends_with(".xhscdn.com")
            } else {
                host == "douyinpic.com"
                    || host.ends_with(".douyinpic.com")
                    || host.ends_with(".byteimg.com")
            };
            if parsed.scheme() != "https" || !allowed {
                bail!("图片来源不是该平台的图片服务器");
            }
            if !images.iter().any(|saved| saved == url) {
                images.push(url.to_owned());
            }
        }
        Ok(Self {
            description: value["description"].as_str().unwrap_or_default().into(),
            author: value["author"].as_str().unwrap_or_default().into(),
            images,
        })
    }
}

pub fn is_xhs(input: &str) -> bool {
    supported_url(input)
        .and_then(|url| reqwest::Url::parse(&url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| {
            ["xiaohongshu.com", "xhslink.com"]
                .iter()
                .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        })
}

fn xhs_id(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let parts: Vec<_> = parsed.path_segments()?.collect();
    parts
        .windows(2)
        .find(|p| {
            matches!(p[0], "explore" | "item")
                && p[1].len() == 24
                && p[1].chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(|p| p[1].to_ascii_lowercase())
}

pub fn probe_xhs(input: &str) -> Result<ExternalVideo> {
    let url = supported_url(input)
        .filter(|u| is_xhs(u))
        .context("请粘贴小红书图文链接或分享文字")?;
    let resolved = if xhs_id(&url).is_some() {
        url
    } else {
        reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(20))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36")
            .build()?.get(&url).send()?.error_for_status()?.url().to_string()
    };
    if !is_xhs(&resolved) || xhs_id(&resolved).is_none() {
        bail!("未定位到小红书笔记，请从分享菜单复制完整链接");
    }
    // Keep the original query: xsec_token is needed to open the shared note.
    let preview = tempfile::NamedTempFile::new()?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let status = command
        .arg("--xhs-gallery")
        .arg(&resolved)
        .arg(preview.path())
        .status()?;
    if status.code() == Some(2) {
        bail!("已取消小红书图文提取");
    }
    if !status.success() {
        bail!("小红书图文窗口读取失败，请重试");
    }
    let message = std::fs::read_to_string(preview.path())?;
    let value: Value = serde_json::from_str(&message)?;
    let gallery = Gallery::from_message(&message, &resolved)?;
    let title = value["title"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or("小红书图文")
        .chars()
        .take(70)
        .collect();
    Ok(ExternalVideo {
        title,
        url: resolved,
        gallery: Some(gallery),
    })
}

fn note_id(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let parts: Vec<_> = parsed.path_segments()?.collect();
    parts
        .windows(2)
        .find(|p| {
            matches!(p[0], "note" | "slides")
                && !p[1].is_empty()
                && p[1].chars().all(|c| c.is_ascii_digit())
        })
        .map(|p| p[1].to_owned())
}

fn gallery_probe(url: &str) -> Result<Option<ExternalVideo>> {
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(20))
        .user_agent("Mozilla/5.0 (Linux; Android 13) AppleWebKit/537.36 Chrome/120.0.0.0 Mobile Safari/537.36").build()?;
    let resolved = if note_id(url).is_some() {
        url.to_owned()
    } else {
        client
            .get(url)
            .send()?
            .error_for_status()?
            .url()
            .to_string()
    };
    let Some(id) = note_id(&resolved) else {
        return Ok(None);
    };
    let canonical = format!("https://www.douyin.com/note/{id}");
    let preview = tempfile::NamedTempFile::new()?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let status = command
        .arg("--douyin-gallery")
        .arg(&canonical)
        .arg(preview.path())
        .status()?;
    if status.code() == Some(2) {
        bail!("已取消获取抖音图文");
    }
    if !status.success() {
        bail!("图文窗口读取失败，请重试");
    }
    let message = std::fs::read_to_string(preview.path())?;
    let gallery = Gallery::from_message(&message, &canonical)?;
    let title = gallery.description.chars().take(70).collect::<String>();
    Ok(Some(ExternalVideo {
        title: if title.is_empty() {
            "抖音图文".into()
        } else {
            title
        },
        url: canonical,
        gallery: Some(gallery),
    }))
}

pub fn download_gallery(
    video: &ExternalVideo,
    output_dir: &Path,
    format: crate::export::TextFormat,
    progress: &dyn Fn(f64),
) -> Result<PathBuf> {
    use std::io::{Read, Write};
    let gallery = video.gallery.as_ref().context("不是图文作品")?;
    std::fs::create_dir_all(output_dir)?;
    let folder = tempfile::Builder::new()
        .prefix(&format!(
            "{}_图文_",
            crate::bili::sanitize_filename(&video.title)
        ))
        .tempdir_in(output_dir)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()?;
    for (index, url) in gallery.images.iter().enumerate() {
        let response = client
            .get(url)
            .header(
                "Referer",
                if crate::douyin_article::bili_url(&video.url).is_some() {
                    "https://www.bilibili.com/"
                } else if is_xhs(&video.url) {
                    "https://www.xiaohongshu.com/"
                } else {
                    "https://www.douyin.com/"
                },
            )
            .send()?
            .error_for_status()?;
        let mut bytes = Vec::new();
        response.take(50_000_001).read_to_end(&mut bytes)?;
        if bytes.len() > 50_000_000 {
            bail!("第 {} 张图片过大", index + 1);
        }
        let extension = image_extension(&bytes).context(format!(
            "第 {} 张图片下载失败，服务器没有返回图片",
            index + 1
        ))?;
        let mut image =
            std::fs::File::create(folder.path().join(format!("{:02}.{extension}", index + 1)))?;
        image.write_all(&bytes)?;
        progress((index + 1) as f64 / (gallery.images.len() + 1) as f64);
    }
    let text = folder
        .path()
        .join(format!("作品文案.{}", format.extension()));
    crate::export::save(
        &text,
        &video.title,
        &format!(
            "{}\n\n作者：{}\n来源：{}",
            gallery.description, gallery.author, video.url
        ),
        format,
    )?;
    progress(1.0);
    Ok(folder.keep())
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else {
        None
    }
}

pub fn find_yt_dlp() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let cwd = std::env::current_dir().ok();

    let mut candidates = Vec::new();
    if let Some(dir) = exe_dir {
        candidates.push(dir.join("tools").join("yt-dlp.exe"));
        candidates.push(dir.join("yt-dlp.exe"));
    }
    if let Some(dir) = cwd {
        candidates.push(dir.join("tools").join("yt-dlp.exe"));
        candidates.push(dir.join("yt-dlp.exe"));
    }

    candidates.into_iter().find(|path| path.is_file())
}

pub fn is_supported(url: &str) -> bool {
    supported_url(url).is_some()
}

pub fn is_douyin(input: &str) -> bool {
    supported_url(input)
        .and_then(|url| reqwest::Url::parse(&url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| {
            ["douyin.com", "iesdouyin.com"]
                .iter()
                .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        })
}

pub fn douyin_cookie_path() -> PathBuf {
    crate::zhihu_login::session_directory().join("douyin-cookies.txt")
}

pub(crate) fn configure_command(command: &mut std::process::Command, url: &str) {
    command.env("PYTHONIOENCODING", "utf-8");
    if crate::youtube::link(url).is_some() {
        if crate::youtube::cookie_path().is_file() {
            command.arg("--cookies").arg(crate::youtube::cookie_path());
        }
        if let Some(tool) = find_yt_dlp() {
            let node = tool.parent().unwrap_or(Path::new(".")).join("node.exe");
            if node.is_file() {
                command
                    .arg("--js-runtimes")
                    .arg(format!("node:{}", node.display()));
            }
        }
    }
    if is_douyin(url) && douyin_cookie_path().is_file() {
        command.arg("--cookies").arg(douyin_cookie_path());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}

/// Douyin's copy/share action includes a caption around the actual link.
pub fn supported_url(input: &str) -> Option<String> {
    for part in input.split(|c: char| c.is_whitespace() || "，。；！【】《》“”".contains(c))
    {
        let Some(start) = part.find("https://").or_else(|| part.find("http://")) else {
            continue;
        };
        let candidate =
            part[start..].trim_end_matches([')', ']', '}', ',', '.', ';', '!', '\'', '"']);
        if supported_host(candidate) {
            let parsed = reqwest::Url::parse(candidate).ok()?;
            if parsed
                .host_str()
                .is_some_and(|host| host == "douyin.com" || host.ends_with(".douyin.com"))
            {
                if let Some((_, id)) = parsed.query_pairs().find(|(key, _)| key == "modal_id") {
                    if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                        return Some(format!("https://www.douyin.com/video/{id}"));
                    }
                }
            }
            return Some(candidate.to_owned());
        }
    }
    None
}

fn supported_host(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    [
        "douyin.com",
        "iesdouyin.com",
        "xiaohongshu.com",
        "xhslink.com",
        "youtube.com",
        "youtu.be",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use super::{image_extension, is_supported, supported_url, Gallery};

    #[test]
    fn gallery_rejects_other_works_and_external_images() {
        let source = "https://www.douyin.com/note/7690633219121260197";
        let mut value = serde_json::json!({"source":source,"description":"作品文案","author":"bro", "images":["https://p3-pc-sign.douyinpic.com/image.webp","https://p3-pc-sign.douyinpic.com/image.webp"]});
        assert_eq!(
            Gallery::from_message(&value.to_string(), source)
                .unwrap()
                .images
                .len(),
            1
        );
        assert!(
            Gallery::from_message(&value.to_string(), "https://www.douyin.com/note/123").is_err()
        );
        value["images"] = serde_json::json!(["https://douyinpic.com.evil.test/image.webp"]);
        assert!(Gallery::from_message(&value.to_string(), source).is_err());
        value["images"] = serde_json::json!([]);
        assert!(Gallery::from_message(&value.to_string(), source).is_err());
    }

    #[test]
    fn image_save_keeps_real_format_and_rejects_error_pages() {
        assert_eq!(image_extension(b"RIFF0000WEBPdata"), Some("webp"));
        assert_eq!(image_extension(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(image_extension(b"<html>Forbidden</html>"), None);
    }

    #[test]
    fn xhs_share_keeps_access_query_and_matches_only_the_requested_note() {
        let source = "https://www.xiaohongshu.com/explore/674051740000000007027a15?xsec_token=test%3D&xsec_source=pc_share";
        assert_eq!(
            supported_url(&format!("笔记标题 http://xhslink.com/a/abc 查看更多")),
            Some("http://xhslink.com/a/abc".into())
        );
        assert_eq!(supported_url(source), Some(source.into()));
        let mut value = serde_json::json!({"source":source,"description":"首段\n末尾文字完整保留","author":"作者", "images":["https://sns-webpic-qc.xhscdn.com/picture1.webp", "https://sns-webpic-qc.xhscdn.com/picture2.webp"]});
        let gallery = Gallery::from_message(&value.to_string(), source).unwrap();
        assert_eq!(gallery.images.len(), 2);
        assert!(gallery.description.ends_with("末尾文字完整保留"));
        assert!(Gallery::from_message(
            &value.to_string(),
            "https://www.xiaohongshu.com/explore/674051740000000007027a16"
        )
        .is_err());
        value["images"] = serde_json::json!(["https://xhscdn.com.evil.test/image.webp"]);
        assert!(Gallery::from_message(&value.to_string(), source).is_err());
        value["source"] =
            "https://xiaohongshu.com.evil.test/explore/674051740000000007027a15".into();
        assert!(Gallery::from_message(&value.to_string(), source).is_err());
    }

    #[test]
    fn extracts_video_url_from_douyin_share_text() {
        assert_eq!(
            supported_url("3.14 复制打开抖音，看看这个视频 https://v.douyin.com/abc123/ 10/05"),
            Some("https://v.douyin.com/abc123/".into())
        );
        assert_eq!(
            supported_url("视频：https://www.douyin.com/video/123。复制此链接"),
            Some("https://www.douyin.com/video/123".into())
        );
        assert!(
            supported_url("https://evil.test/?redirect=https://www.douyin.com/video/123").is_none()
        );
    }

    #[test]
    fn only_supported_hosts_are_external_videos() {
        assert!(is_supported("https://www.douyin.com/video/123"));
        assert!(is_supported("https://xhslink.com/abc"));
        assert!(!is_supported("https://douyin.com.evil.test/video/123"));
        assert!(!is_supported("https://evil.test/?q=douyin.com"));
    }

    #[test]
    fn normalizes_douyin_selection_links() {
        assert_eq!(
            supported_url("https://www.douyin.com/jingxuan?modal_id=7673787196373060900"),
            Some("https://www.douyin.com/video/7673787196373060900".into())
        );
        assert!(!is_supported(
            "https://douyin.com.evil.test/jingxuan?modal_id=123"
        ));
    }
}

pub fn probe(url: &str, tool: &Path) -> Result<ExternalVideo> {
    let url = supported_url(url).context("未找到支持的视频链接")?;
    if is_douyin(&url) {
        if let Some(gallery) = gallery_probe(&url)? {
            return Ok(gallery);
        }
    }
    let mut command = std::process::Command::new(tool);
    configure_command(&mut command, &url);
    let output = command
        .args([
            "--dump-single-json",
            "--no-warnings",
            "--no-playlist",
            "--socket-timeout",
            "15",
            "--retries",
            "1",
        ])
        .arg(&url)
        .output()
        .context("启动 yt-dlp 失败")?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        if error.contains("Fresh cookies") {
            bail!("抖音要求有效的访问 Cookie。请点右上角「抖音登录」，访问或登录后关闭窗口，再重新获取。已有会话时仍失败，说明当前下载工具暂时无法解析该视频。");
        }
        bail!("{error}");
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    Ok(ExternalVideo {
        title: value["title"].as_str().unwrap_or("未命名视频").to_string(),
        url,
        gallery: None,
    })
}

pub fn download(
    url: &str,
    tool: &Path,
    output_dir: &Path,
    progress: &dyn Fn(f64),
) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)?;
    let mut command = std::process::Command::new(tool);
    configure_command(&mut command, url);
    let mut child = command
        .args([
            "--newline",
            "--no-playlist",
            "--no-warnings",
            "--no-simulate",
            "-f",
            "bv*+ba/b",
            "--merge-output-format",
            "mp4",
            "-P",
        ])
        .arg(output_dir)
        .arg("--print")
        .arg("after_move:filepath")
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("启动 yt-dlp 失败")?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("无法读取 yt-dlp 输出"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("无法读取 yt-dlp 错误输出"))?;
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        for line in BufReader::new(stderr).lines().flatten() {
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let mut saved_path: Option<PathBuf> = None;
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let line = line?;
        if let Some(pct) = parse_progress(&line) {
            progress(pct);
        } else if line.trim_end().ends_with(".mp4") {
            saved_path = Some(PathBuf::from(line.trim_end()));
        }
    }

    let status = child.wait()?;
    let stderr_text = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        bail!("视频下载失败: {stderr_text}");
    }
    saved_path.ok_or_else(|| anyhow!("下载完成，但没有返回文件路径"))
}

fn parse_progress(line: &str) -> Option<f64> {
    let marker = "[download] ";
    let idx = line.find(marker)? + marker.len();
    let rest = line[idx..].trim_start();
    let pct_end = rest.find('%')?;
    let pct: f64 = rest[..pct_end].trim().parse().ok()?;
    Some((pct / 100.0).clamp(0.0, 1.0))
}
