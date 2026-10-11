use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
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
            let site = if xhs {
                GallerySite::Xhs
            } else {
                GallerySite::Douyin
            };
            if !image_url_allowed(&parsed, site) {
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
    let source = reqwest::Url::parse(&video.url).context("图文来源无效")?;
    if !source.username().is_empty() || source.password().is_some() || source.port().is_some() {
        bail!("图文来源地址无效");
    }
    let site = if crate::douyin_article::bili_url(&video.url).is_some() {
        GallerySite::Bili
    } else if is_xhs(&video.url) {
        GallerySite::Xhs
    } else if is_douyin(&video.url) {
        GallerySite::Douyin
    } else {
        bail!("不支持此图文来源");
    };
    if gallery.images.is_empty() || gallery.images.len() > 200 {
        bail!("图文图片数量无效，请重新获取");
    }
    for url in &gallery.images {
        let url = reqwest::Url::parse(url).context("图文图片地址无效")?;
        if !image_url_allowed(&url, site) {
            bail!("图片来源不是该平台的图片服务器");
        }
    }
    std::fs::create_dir_all(output_dir)?;
    let folder = tempfile::Builder::new()
        .prefix(&format!(
            "{}_图文_",
            crate::bili::sanitize_filename(&video.title)
        ))
        .tempdir_in(output_dir)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 8 {
                attempt.error("图片重定向过多")
            } else if image_url_allowed(attempt.url(), site) {
                attempt.follow()
            } else {
                attempt.error("图片跳转到了非支持的服务器")
            }
        }))
        .build()?;
    for (index, url) in gallery.images.iter().enumerate() {
        let response = client
            .get(url)
            .header(
                "Referer",
                match site {
                    GallerySite::Bili => "https://www.bilibili.com/",
                    GallerySite::Xhs => "https://www.xiaohongshu.com/",
                    GallerySite::Douyin => "https://www.douyin.com/",
                },
            )
            .send()?
            .error_for_status()?;
        if !image_url_allowed(response.url(), site) {
            bail!("第 {} 张图片的最终来源不受支持", index + 1);
        }
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

#[derive(Clone, Copy)]
enum GallerySite {
    Bili,
    Xhs,
    Douyin,
}

fn image_url_allowed(url: &reqwest::Url, site: GallerySite) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    let host = url.host_str().unwrap_or_default();
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    match site {
        GallerySite::Bili => under("hdslb.com"),
        GallerySite::Xhs => under("xhscdn.com"),
        GallerySite::Douyin => under("douyinpic.com") || under("byteimg.com"),
    }
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
    // User-wide settings may redirect files, enable audio-only extraction or
    // execute post-download commands. This app always supplies its own options.
    command.args(["--ignore-config", "--no-config-locations", "--no-colors"]);
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
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.port().is_some() {
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
    use super::*;

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

    #[test]
    fn explicit_json_marker_keeps_unicode_and_does_not_confuse_log_messages() {
        let path = r"C:\视频输出\作品 [id].webm";
        let line = format!("__SHIWEN_FILE__{}", serde_json::to_string(path).unwrap());
        assert_eq!(parse_output_path(&line).unwrap(), Some(PathBuf::from(path)));
        assert!(parse_output_path("[Merger] Merging formats into video.mp4")
            .unwrap()
            .is_none());
        assert!(parse_output_path("C:/ordinary-log.mp4").unwrap().is_none());
        assert!(parse_output_path("__SHIWEN_FILE__null").is_err());
        assert!(parse_output_path("__SHIWEN_FILE__\"\"").is_err());
        assert_eq!(parse_progress("[download] 42.5% of 20 MiB"), Some(0.425));
        assert!(parse_progress("[download] NaN%").is_none());
    }

    #[test]
    fn staged_result_rejects_escape_missing_empty_and_non_video_files() {
        let directory = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir_in(directory.path()).unwrap();
        let outside = directory.path().join("other.mp4");
        std::fs::write(&outside, b"old video").unwrap();
        assert!(checked_staged_file(&outside, staging.path()).is_err());
        assert!(checked_staged_file(Path::new("../other.mp4"), staging.path()).is_err());
        assert!(checked_staged_file(Path::new("missing.mp4"), staging.path()).is_err());
        assert!(checked_staged_file(staging.path(), staging.path()).is_err());
        let empty = staging.path().join("empty.mp4");
        std::fs::write(&empty, []).unwrap();
        assert!(checked_staged_file(&empty, staging.path()).is_err());
        let info = staging.path().join("video.info.json");
        std::fs::write(&info, b"{}").unwrap();
        assert!(checked_staged_file(&info, staging.path()).is_err());
        let media = staging.path().join("final.WEBM");
        std::fs::write(&media, b"finished video placeholder").unwrap();
        assert_eq!(
            checked_staged_file(Path::new("final.WEBM"), staging.path()).unwrap(),
            media.canonicalize().unwrap()
        );
        let staging_path = staging.path().to_owned();
        drop(staging);
        assert!(!staging_path.exists());
        assert_eq!(std::fs::read(outside).unwrap(), b"old video");
    }

    #[test]
    fn publication_preserves_existing_results_and_uses_safe_windows_names() {
        let directory = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir_in(directory.path()).unwrap();
        let existing = directory.path().join("同名视频.mp4");
        std::fs::write(&existing, b"keep original").unwrap();
        std::fs::create_dir(directory.path().join("同名视频 (1).mp4")).unwrap();
        let staged = staging.path().join("同名视频.MP4");
        std::fs::write(&staged, b"new complete video").unwrap();
        let verified = checked_staged_file(&staged, staging.path()).unwrap();
        let published = publish_video(&verified, directory.path()).unwrap();
        assert_eq!(published.file_name().unwrap(), "同名视频 (2).mp4");
        assert_eq!(std::fs::read(&existing).unwrap(), b"keep original");
        assert_eq!(std::fs::read(&published).unwrap(), b"new complete video");
        assert!(!staged.exists());
        assert_eq!(safe_video_stem("CON"), "_CON");
        assert_eq!(safe_video_stem("NUL.data"), "_NUL.data");
        assert_eq!(safe_video_stem("COM1"), "_COM1");
        assert_eq!(safe_video_stem("LPT²"), "_LPT²");
        assert_eq!(safe_video_stem(" 标题:第一集. "), "标题_第一集");
    }

    #[test]
    fn downloader_configuration_is_isolated_and_error_logs_stay_bounded() {
        let mut command = std::process::Command::new("yt-dlp");
        configure_command(&mut command, "https://www.xiaohongshu.com/explore/test");
        let arguments: Vec<_> = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(arguments.contains(&"--ignore-config".to_owned()));
        assert!(arguments.contains(&"--no-config-locations".to_owned()));
        let error = format!("{}final failure", "x".repeat(40_000));
        let tail = error_tail(error.as_bytes());
        assert!(tail.len() <= 16 * 1024);
        assert!(tail.ends_with("final failure"));
    }

    #[test]
    fn gallery_redirect_checks_preserve_platform_cdn_boundaries() {
        let allowed = |url: &str, site| image_url_allowed(&reqwest::Url::parse(url).unwrap(), site);
        assert!(allowed("https://i0.hdslb.com/bfs/a.jpg", GallerySite::Bili));
        assert!(allowed(
            "https://sns-webpic-qc.xhscdn.com/picture.webp",
            GallerySite::Xhs
        ));
        assert!(allowed(
            "https://p3-pc-sign.douyinpic.com/image.webp",
            GallerySite::Douyin
        ));
        assert!(allowed(
            "https://p3.byteimg.com/image.webp",
            GallerySite::Douyin
        ));
        assert!(allowed(
            "https://p3.byteimg.com:443/image.webp",
            GallerySite::Douyin
        ));
        for invalid in [
            "http://p3.byteimg.com/image.webp",
            "https://p3.byteimg.com.evil.test/image.webp",
            "https://user:pass@p3.byteimg.com/image.webp",
            "https://p3.byteimg.com:8443/image.webp",
            "https://127.0.0.1/image.webp",
        ] {
            assert!(!allowed(invalid, GallerySite::Douyin), "{invalid}");
        }
        assert!(!allowed(
            "https://p3.byteimg.com/image.webp",
            GallerySite::Xhs
        ));
        assert!(!allowed(
            "https://sns-webpic-qc.xhscdn.com/picture.webp",
            GallerySite::Bili
        ));
        let source = "https://www.douyin.com/note/123";
        for invalid in [
            "https://user:secret@p3.byteimg.com/image.webp",
            "https://p3.byteimg.com:8443/image.webp",
        ] {
            let message = serde_json::json!({"source":source,"images":[invalid]});
            assert!(Gallery::from_message(&message.to_string(), source).is_err());
        }
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
            "--extractor-retries",
            "1",
        ])
        .arg(&url)
        .stdin(Stdio::null())
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
    let output_dir = output_dir.canonicalize().context("无法打开输出目录")?;
    // Keep every partial file away from previously exported results. TempDir
    // removes fragments and failed postprocessing outputs on every error path.
    let staging = tempfile::Builder::new()
        .prefix(".shiwen-download-")
        .tempdir_in(&output_dir)?;
    let mut command = std::process::Command::new(tool);
    configure_command(&mut command, url);
    let mut child = command
        .args([
            "--newline",
            "--progress",
            "--no-playlist",
            "--no-warnings",
            "--no-simulate",
            "--windows-filenames",
            "--socket-timeout",
            "20",
            "--retries",
            "2",
            "--fragment-retries",
            "2",
            "--extractor-retries",
            "1",
            "--file-access-retries",
            "1",
            "--abort-on-unavailable-fragments",
            "-f",
            "bv*+ba/b",
            "--merge-output-format",
            "mp4",
            "--remux-video",
            "mp4",
            "-P",
        ])
        .arg(staging.path())
        .args([
            "-o",
            "%(title).100B [%(id).32B].%(ext)s",
            "--print",
            "after_move:__SHIWEN_FILE__%(filepath)j",
        ])
        .arg(url)
        .stdin(Stdio::null())
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
    let stderr_handle = std::thread::spawn(move || error_tail(stderr));

    let mut saved_paths = Vec::new();
    let mut read_error = None;
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                read_error = Some(anyhow!(error).context("无法读取下载进度"));
                break;
            }
        };
        if let Some(pct) = parse_progress(&line) {
            // Leave room for audio/video merging and final publication.
            progress(pct * 0.95);
        }
        match parse_output_path(&line) {
            Ok(Some(path)) => {
                if !saved_paths.contains(&path) {
                    saved_paths.push(path);
                }
            }
            Ok(None) => {}
            Err(error) => {
                read_error = Some(error);
                break;
            }
        }
    }

    if read_error.is_some() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let stderr_text = stderr_handle.join().unwrap_or_default();
    if let Some(error) = read_error {
        return Err(error);
    }
    if !status.success() {
        bail!("视频下载失败: {stderr_text}");
    }
    if saved_paths.len() != 1 {
        bail!(if saved_paths.is_empty() {
            "下载未返回视频文件，请重新获取链接后重试"
        } else {
            "该链接包含多个视频，请打开其中一个视频的独立链接再下载"
        });
    }
    let path = checked_staged_file(&saved_paths[0], staging.path())?;
    progress(0.98);
    let published = publish_video(&path, &output_dir)?;
    progress(1.0);
    Ok(published)
}

fn error_tail(mut reader: impl Read) -> String {
    const LIMIT: usize = 16 * 1024;
    let mut tail = Vec::new();
    let mut chunk = [0_u8; 4096];
    while let Ok(length) = reader.read(&mut chunk) {
        if length == 0 {
            break;
        }
        tail.extend_from_slice(&chunk[..length]);
        if tail.len() > LIMIT {
            tail.drain(..tail.len() - LIMIT);
        }
    }
    String::from_utf8_lossy(&tail).trim().to_owned()
}

fn parse_output_path(line: &str) -> Result<Option<PathBuf>> {
    let Some(value) = line.strip_prefix("__SHIWEN_FILE__") else {
        return Ok(None);
    };
    let path: String = serde_json::from_str(value).context("下载组件返回的文件位置无法读取")?;
    if path.is_empty() || path.contains('\0') {
        bail!("下载组件返回了无效的文件位置");
    }
    Ok(Some(PathBuf::from(path)))
}

fn checked_staged_file(reported: &Path, staging: &Path) -> Result<PathBuf> {
    let staging = staging.canonicalize()?;
    let candidate = if reported.is_absolute() {
        reported.to_owned()
    } else {
        staging.join(reported)
    };
    let path = candidate
        .canonicalize()
        .context("下载完成，但视频文件不存在")?;
    if !path.starts_with(&staging) || path == staging {
        bail!("下载组件返回了暂存目录以外的文件，已停止保存");
    }
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!("下载结果为空，未保存无效文件");
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "mp4" | "m4v" | "webm" | "mkv" | "mov" | "flv" | "avi" | "ts" | "3gp"
    ) {
        bail!("下载组件没有返回支持的视频文件，未保存无效结果");
    }
    Ok(path)
}

fn safe_video_stem(input: &str) -> String {
    let mut name = crate::bili::sanitize_filename(input);
    let device = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let numbered_device = device
        .strip_prefix("COM")
        .or_else(|| device.strip_prefix("LPT"))
        .is_some_and(|number| {
            matches!(
                number,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        });
    if matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$") || numbered_device {
        name.insert(0, '_');
    }
    name
}

fn publish_video(staged: &Path, output_dir: &Path) -> Result<PathBuf> {
    let extension = staged
        .extension()
        .and_then(|value| value.to_str())
        .context("视频缺少文件格式")?
        .to_ascii_lowercase();
    let stem = safe_video_stem(&staged.file_stem().unwrap_or_default().to_string_lossy());
    // TempPath takes ownership only after the staging boundary has been checked.
    // persist_noclobber moves within the same filesystem without replacing an
    // earlier export, even if another operation races to create the same name.
    let mut source = tempfile::TempPath::try_from_path(staged)?;
    for index in 0..10_000 {
        let filename = if index == 0 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem} ({index}).{extension}")
        };
        let destination = output_dir.join(filename);
        if destination.exists() {
            continue;
        }
        match source.persist_noclobber(&destination) {
            Ok(()) => return Ok(destination),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                source = error.path
            }
            Err(error) => return Err(error.error).context("视频已下载，但保存到输出目录失败"),
        }
    }
    bail!("同名视频过多，请更换输出目录")
}

fn parse_progress(line: &str) -> Option<f64> {
    let marker = "[download] ";
    let idx = line.find(marker)? + marker.len();
    let rest = line[idx..].trim_start();
    let pct_end = rest.find('%')?;
    let pct: f64 = rest[..pct_end].trim().parse().ok()?;
    pct.is_finite().then(|| (pct / 100.0).clamp(0.0, 1.0))
}
