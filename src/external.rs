use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Clone, Debug)]
pub struct ExternalVideo {
    pub title: String,
    pub url: String,
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

fn configure_command(command: &mut std::process::Command, url: &str) {
    command.env("PYTHONIOENCODING", "utf-8");
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
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use super::{is_supported, supported_url};

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
    let url = supported_url(url).context("未找到抖音或小红书视频链接")?;
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
        bail!("解析链接失败: {error}");
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    Ok(ExternalVideo {
        title: value["title"].as_str().unwrap_or("未命名视频").to_string(),
        url,
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
