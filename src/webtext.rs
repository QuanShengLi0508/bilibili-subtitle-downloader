use crate::bili::sanitize_filename;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn open(url: &str) -> Result<()> {
    webbrowser::open(url).context("打开浏览器失败")
}

pub fn save_clipboard_text(output_dir: &Path) -> Result<PathBuf> {
    let text = arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .context("读取剪贴板失败")?;
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("剪贴板是空的，请先在知乎页面复制文本");
    }

    std::fs::create_dir_all(output_dir)?;
    let title = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("知乎文本");
    let name = sanitize_filename(title);
    let path = output_dir.join(format!("{name}.txt"));
    std::fs::write(&path, text)?;
    Ok(path)
}
