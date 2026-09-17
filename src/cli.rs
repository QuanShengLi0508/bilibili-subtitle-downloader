use crate::bili::{lines_to_srt, lines_to_txt, sanitize_filename, Client};
use anyhow::Result;
use std::path::PathBuf;

pub fn run(input: &str) -> Result<()> {
        let (debug, input) = if let Some(rest) = input.strip_prefix("--debug ") {
            (true, rest)
        } else {
            (false, input)
        };
        let (want_srt, input) = if let Some(rest) = input.strip_prefix("--srt ") {
            (true, rest)
        } else {
            (false, input)
        };
    let client = Client::new();
    println!("正在解析链接...");
    let (video, page) = client.fetch_video(input)?;
    println!("视频: {} (共 {} 个分P)", video.title, video.pages.len());

    println!("正在获取字幕列表...");
    let tracks = client.fetch_tracks(&video, page)?;
    if debug {
        println!("{}", client.debug_tracks_json(&video, page)?);
        return Ok(());
    }
    if tracks.is_empty() {
        eprintln!("该视频没有可下载的字幕（可能未上传CC字幕，或AI字幕需要登录才能获取）");
        std::process::exit(2);
    }
    for t in &tracks {
        println!("可用字幕: {} [{}]", t.lan_doc, t.lan);
    }

    let track = &tracks[0];
    println!("正在下载字幕: {} ...", track.lan_doc);
    let lines = client.fetch_subtitle_body(&track.url)?;
    let content = if want_srt { lines_to_srt(&lines) } else { lines_to_txt(&lines) };
    let ext = if want_srt { "srt" } else { "txt" };

    let mut name = sanitize_filename(&video.title);
    if page > 1 {
        name.push_str(&format!("_P{page}"));
    }
    name.push_str(&format!("_{}.{}", track.lan, ext));
    let dir = output_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&name);
    std::fs::write(&path, content)?;
    println!("已保存: {}", path.display());
    Ok(())
}

pub fn output_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("字幕输出")
}
