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
    let lower = url.to_lowercase();
    lower.contains("douyin.com")
        || lower.contains("iesdouyin.com")
        || lower.contains("xiaohongshu.com")
        || lower.contains("xhslink.com")
}

pub fn probe(url: &str, tool: &Path) -> Result<ExternalVideo> {
    let output = std::process::Command::new(tool)
        .args(["--dump-single-json", "--no-warnings", "--no-playlist"])
        .arg(url)
        .output()
        .context("启动 yt-dlp 失败")?;
    if !output.status.success() {
        bail!("解析链接失败: {}", String::from_utf8_lossy(&output.stderr));
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    Ok(ExternalVideo {
        title: value["title"].as_str().unwrap_or("未命名视频").to_string(),
        url: url.to_string(),
    })
}

pub fn download(
    url: &str,
    tool: &Path,
    output_dir: &Path,
    progress: &dyn Fn(f64),
) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)?;
    let mut child = std::process::Command::new(tool)
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
