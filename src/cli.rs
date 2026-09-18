use crate::bili::{lines_to_srt, lines_to_txt, sanitize_filename, Client, VideoInfo, VideoStream};
use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;

pub fn run(input: &str) -> Result<()> {
    // 命令行模式:
    //   <链接>               下载字幕 TXT（--srt 则为 SRT）
    //   --debug <链接>       打印字幕接口原始返回
    //   --streams <链接>     打印可用画质列表
    //   --video <链接> [qn]  下载视频（默认最高画质）
    let mut debug = false;
    let mut want_srt = false;
    let mut streams_only = false;
    let mut video_mode = false;
    let mut video_qn: Option<u64> = None;
    let mut input = input.trim().to_string();
    loop {
        if let Some(rest) = input.strip_prefix("--debug ") {
            debug = true;
            input = rest.trim().to_string();
        } else if let Some(rest) = input.strip_prefix("--srt ") {
            want_srt = true;
            input = rest.trim().to_string();
        } else if let Some(rest) = input.strip_prefix("--streams ") {
            streams_only = true;
            input = rest.trim().to_string();
        } else if let Some(rest) = input.strip_prefix("--video ") {
            video_mode = true;
            let arg0 = rest.split_whitespace().next().unwrap_or("").to_string();
            let arg1 = rest.split_whitespace().nth(1).map(String::from);
            input = arg0;
            video_qn = arg1.and_then(|s| s.parse().ok());
        } else {
            break;
        }
    }

    let client = Client::new();
    println!("正在解析链接...");
    let (video, page) = client.fetch_video(&input)?;
    println!("视频: {} (共 {} 个分P)", video.title, video.pages.len());

    println!("正在获取字幕列表...");
    let tracks = client.fetch_tracks(&video, page)?;
    if debug {
        println!("{}", client.debug_tracks_json(&video, page)?);
        return Ok(());
    }
    for t in &tracks {
        println!("可用字幕: {} [{}]", t.lan_doc, t.lan);
    }

    println!("正在获取画质列表...");
    let streams = client.fetch_streams(&video, page).unwrap_or_default();
    for s in &streams {
        println!("可用画质: {} [{}]", s.label, s.quality_id);
    }
    if streams_only {
        return Ok(());
    }

    if video_mode {
        return download_video(&client, &video, page, &streams, video_qn);
    }

    if tracks.is_empty() {
        eprintln!("该视频没有可下载的字幕（可能未上传CC字幕，或AI字幕需要登录才能获取）");
        std::process::exit(2);
    }

    let track = &tracks[0];
    println!("正在下载字幕: {} ...", track.lan_doc);
    let lines = client.fetch_subtitle_body(&track.url)?;
    let content = if want_srt {
        lines_to_srt(&lines)
    } else {
        lines_to_txt(&lines)
    };
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

fn download_video(
    client: &Client,
    video: &VideoInfo,
    page: usize,
    streams: &[VideoStream],
    qn: Option<u64>,
) -> Result<()> {
    if streams.is_empty() {
        bail!("没有可用的视频流");
    }
    let stream = match qn {
        None => streams.first().unwrap(),
        Some(q) => streams
            .iter()
            .find(|s| s.quality_id == q)
            .ok_or_else(|| anyhow!("没有指定的清晰度 {q}"))?,
    };
    println!("下载画质: {}", stream.label);
    if stream.audio_url.is_some() && !Client::ffmpeg_available() {
        bail!("未检测到 ffmpeg，无法合并音视频。请先安装: winget install Gyan.FFmpeg");
    }

    let mut name = sanitize_filename(&video.title);
    if page > 1 {
        name.push_str(&format!("_P{page}"));
    }
    name.push_str(&format!("_{}.mp4", stream.quality_id));
    let dir = output_dir();
    std::fs::create_dir_all(&dir)?;
    let out_path = dir.join(&name);
    let video_tmp = dir.join(format!("{name}.video.tmp"));
    let audio_tmp = dir.join(format!("{name}.audio.tmp"));

    match &stream.audio_url {
        Some(audio) => {
            println!("下载画面...");
            client.download_to_file(&stream.video_url, &video_tmp, &|p| {
                print!("\r进度: {:.1}%  ", p * 100.0);
            })?;
            println!();
            println!("下载音频...");
            client.download_to_file(audio, &audio_tmp, &|p| {
                print!("\r进度: {:.1}%  ", p * 100.0);
            })?;
            println!();
            println!("合并音视频...");
            let status = std::process::Command::new("ffmpeg")
                .args(["-y", "-i"])
                .arg(&video_tmp)
                .args(["-i"])
                .arg(&audio_tmp)
                .args(["-c", "copy"])
                .arg(&out_path)
                .output()?;
            if !status.status.success() {
                bail!(
                    "ffmpeg 合并失败: {}",
                    String::from_utf8_lossy(&status.stderr)
                );
            }
            let _ = std::fs::remove_file(&video_tmp);
            let _ = std::fs::remove_file(&audio_tmp);
        }
        None => {
            client.download_to_file(&stream.video_url, &out_path, &|p| {
                print!("\r进度: {:.1}%  ", p * 100.0);
            })?;
            println!();
        }
    }
    println!("已保存: {}", out_path.display());
    Ok(())
}

pub fn output_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("字幕输出")
}
