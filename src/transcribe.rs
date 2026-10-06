use crate::bili::sanitize_filename;
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
mod speakers;
pub use speakers::Options as SpeakerOptions;

pub fn speakers_available() -> bool {
    speakers::resources().is_some()
}

pub fn run_bili(
    video: &crate::bili::VideoInfo,
    page: usize,
    language: &str,
    output_dir: &Path,
    options: SpeakerOptions,
    stage: &dyn Fn(&str),
    progress: &dyn Fn(f64),
) -> Result<Vec<PathBuf>> {
    let model = find_default_model().context("内置识别资源缺失，请重新安装拾文")?;
    if options.enabled && !speakers_available() {
        bail!("多人识别资源缺失，请重新安装新版拾文");
    }
    let client = crate::bili::Client::new();
    stage("正在获取当前分P的独立音频…");
    let streams = client.fetch_streams(video, page)?;
    let audio = streams
        .iter()
        .find_map(|stream| stream.audio_url.as_deref())
        .context("当前视频未提供独立音频，无法使用此方式；可尝试登录后重新获取")?;
    let temp = tempfile::tempdir()?;
    let title = sanitize_filename(&format!("{}_P{page}", video.title));
    let media = temp.path().join(format!("{title}.m4a"));
    stage("正在下载音频（不下载视频画面）…");
    client.download_to_file(audio, &media, progress)?;
    let mut paths = run(&media, &model, language, output_dir, options, stage)?;
    let saved_audio = output_dir.join(format!("{title}_音频.m4a"));
    let mut audio_file = tempfile::NamedTempFile::new_in(output_dir)?;
    std::io::copy(&mut std::fs::File::open(&media)?, audio_file.as_file_mut())?;
    audio_file.as_file().sync_all()?;
    audio_file
        .persist(&saved_audio)
        .context("保存独立音频失败，文件可能正在使用")?;
    paths.push(saved_audio);
    Ok(paths)
}

fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

fn check(result: std::process::Output, stage: &str) -> Result<()> {
    if !result.status.success() {
        bail!("{stage}失败：{}", String::from_utf8_lossy(&result.stderr));
    }
    Ok(())
}
fn save_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("输出目录不存在")?;
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)
        .context("保存结果失败，文件可能正在使用")?;
    Ok(())
}

#[derive(Debug)]
struct Cue {
    start: f64,
    end: f64,
    label: String,
    text: String,
}

fn read_cues(json: &str, turn: &speakers::Turn) -> Result<Vec<Cue>> {
    let data: serde_json::Value = serde_json::from_str(json).context("转写结果无法读取")?;
    let entries = data["transcription"]
        .as_array()
        .context("转写结果缺少文字时间段")?;
    let mut cues = Vec::new();
    for entry in entries {
        let text = entry["text"].as_str().context("转写正文缺失")?.trim();
        if text.is_empty() || text == "[BLANK_AUDIO]" {
            continue;
        }
        let start = entry["offsets"]["from"]
            .as_u64()
            .context("转写起始时间缺失")? as f64
            / 1000.0;
        let end = entry["offsets"]["to"]
            .as_u64()
            .context("转写结束时间缺失")? as f64
            / 1000.0;
        if end < start {
            bail!("转写时间段异常");
        }
        let start = (turn.start + start).min(turn.end);
        let end = (turn.start + end).min(turn.end);
        if end <= start {
            bail!("转写时间超出声音片段，请重新识别");
        }
        cues.push(Cue {
            start,
            end,
            label: turn.label(),
            text: text.into(),
        });
    }
    Ok(cues)
}

fn speaker_documents(cues: &[Cue], speaker_count: usize) -> (String, String) {
    let mut body=format!("识别到 {speaker_count} 位说话人（声音分组，编号按首次出现顺序）\n\n自动结果可能误分，短句、背景音乐和重叠说话请核对；不识别真实姓名。\n\n");
    let mut srt = String::new();
    for (index, cue) in cues.iter().enumerate() {
        let start = crate::bili::format_srt_time(cue.start);
        let end = crate::bili::format_srt_time(cue.end);
        body.push_str(&format!(
            "## {} | {} → {}\n\n{}\n\n",
            cue.label,
            start.replace(',', "."),
            end.replace(',', "."),
            cue.text
        ));
        srt.push_str(&format!(
            "{}\n{start} --> {end}\n[{}] {}\n\n",
            index + 1,
            cue.label,
            cue.text
        ));
    }
    (body, srt)
}

fn run_speakers(
    media: &Path,
    model: &Path,
    language: &str,
    output_dir: &Path,
    options: SpeakerOptions,
    stage: &dyn Fn(&str),
) -> Result<Vec<PathBuf>> {
    let cli = find_whisper_cli().context("识别组件缺失，请重新安装拾文")?;
    if !media.is_file() || !model.is_file() {
        bail!("音视频文件或内置识别资源不存在");
    }
    if !speakers_available() {
        bail!("多人识别资源缺失，请重新安装新版拾文");
    }
    let temp = tempfile::tempdir()?;
    let wav = temp.path().join("audio.wav");
    std::fs::copy(model, temp.path().join("model.bin"))?;
    stage("正在转换音频…");
    check(
        command("ffmpeg")
            .args(["-y", "-i"])
            .arg(media)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
            .arg(&wav)
            .output()
            .context("启动音频转换失败")?,
        "音频转换",
    )?;
    stage("正在区分说话人…");
    let turns = speakers::detect(&wav, options)?;
    let count = turns
        .iter()
        .flat_map(|turn| turn.speakers.iter())
        .copied()
        .max()
        .unwrap_or(0);
    let mut cues = Vec::new();
    for (index, turn) in turns.iter().enumerate() {
        stage(&format!(
            "已分出 {count} 位说话人 · 正在转写 {}/{} 段",
            index + 1,
            turns.len()
        ));
        let clip = temp.path().join("clip.wav");
        check(
            command("ffmpeg")
                .args(["-y", "-i"])
                .arg(&wav)
                .arg("-ss")
                .arg(format!("{:.3}", turn.start))
                .arg("-t")
                .arg(format!("{:.3}", turn.end - turn.start))
                .args(["-c:a", "pcm_s16le"])
                .arg(&clip)
                .output()?,
            "分段音频准备",
        )?;
        let base = temp.path().join("recognized");
        check(
            command(&cli)
                .current_dir(temp.path())
                .args(["-m", "model.bin", "-f", "clip.wav"])
                .args(["-l", language, "-oj", "-of"])
                .arg("recognized")
                .output()
                .context("启动文字识别失败")?,
            "文字识别",
        )?;
        cues.extend(read_cues(
            &std::fs::read_to_string(base.with_extension("json"))?,
            turn,
        )?);
    }
    if cues.is_empty() {
        bail!("检测到声音但没有识别出文字，未生成转写文件");
    }
    let title = format!(
        "{}_多人转写",
        sanitize_filename(&media.file_stem().unwrap_or_default().to_string_lossy())
    );
    let (body, srt) = speaker_documents(&cues, count);
    let txt = output_dir.join(format!("{title}.txt"));
    let srt_path = output_dir.join(format!("{title}.srt"));
    crate::export::save(&txt, &title, &body, crate::export::TextFormat::Txt)?;
    save_bytes(&srt_path, srt.as_bytes())?;
    Ok(vec![txt, srt_path])
}

pub fn find_whisper_cli() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let cwd = std::env::current_dir().ok();

    let mut dirs = Vec::new();
    if let Some(dir) = exe_dir {
        dirs.push(dir.clone());
        dirs.push(dir.join("whisper"));
    }
    if let Some(dir) = cwd {
        dirs.push(dir.clone());
        dirs.push(dir.join("whisper"));
    }

    dirs.iter()
        .map(|dir| dir.join("whisper-cli.exe"))
        .find(|path| path.is_file())
}

pub fn find_default_model() -> Option<PathBuf> {
    let names = [
        "ggml-base-q5_1.bin",
        "ggml-base.bin",
        "ggml-small-q5_1.bin",
        "ggml-small.bin",
    ];
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let cwd = std::env::current_dir().ok();

    let mut dirs = Vec::new();
    if let Some(dir) = exe_dir {
        dirs.push(dir.join("whisper"));
        dirs.push(dir);
    }
    if let Some(dir) = cwd {
        dirs.push(dir.join("whisper"));
        dirs.push(dir);
    }

    for dir in dirs {
        for name in names {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

pub fn run(
    media: &Path,
    model: &Path,
    language: &str,
    output_dir: &Path,
    options: SpeakerOptions,
    stage: &dyn Fn(&str),
) -> Result<Vec<PathBuf>> {
    if options.enabled {
        return run_speakers(media, model, language, output_dir, options, stage);
    }
    let Some(cli) = find_whisper_cli() else {
        bail!("未找到 whisper-cli.exe。请把它放在程序同目录或 whisper 文件夹里。");
    };
    if !model.is_file() {
        bail!("未找到 Whisper 模型文件");
    }
    if !media.is_file() {
        bail!("音频/视频文件不存在");
    }

    std::fs::create_dir_all(output_dir)?;
    let stem = media
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "transcript".to_string());
    let base_name = sanitize_filename(&stem);
    let temp = tempfile::tempdir()?;
    std::fs::copy(model, temp.path().join("model.bin"))?;
    let out_base = temp.path().join("recognized");
    let wav_path = temp.path().join("audio.wav");

    let result = (|| -> Result<Vec<PathBuf>> {
        stage("正在转换音频…");
        let convert = command("ffmpeg")
            .arg("-y")
            .arg("-i")
            .arg(media)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
            .arg(&wav_path)
            .output()
            .context("启动 ffmpeg 失败")?;
        if !convert.status.success() {
            bail!(
                "ffmpeg 转换音频失败: {}",
                String::from_utf8_lossy(&convert.stderr)
            );
        }

        stage("正在本地识别文字…");
        let result = command(&cli)
            .current_dir(temp.path())
            .args(["-m", "model.bin", "-f", "audio.wav"])
            .args(["-l", language, "-otxt", "-osrt"])
            .arg("-of")
            .arg("recognized")
            .output()
            .context("启动 whisper-cli 失败")?;

        if !result.status.success() {
            bail!(
                "Whisper 识别失败: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }

        let txt_path = out_base.with_extension("txt");
        let srt_path = out_base.with_extension("srt");
        if !txt_path.is_file() {
            bail!("Whisper 没有生成 TXT 文件");
        }
        if !srt_path.is_file() {
            bail!("Whisper 没有生成 SRT 文件");
        }
        let txt = output_dir.join(format!("{base_name}_转写.txt"));
        let srt = output_dir.join(format!("{base_name}_转写.srt"));
        save_bytes(&txt, &std::fs::read(txt_path)?)?;
        save_bytes(&srt, &std::fs::read(srt_path)?)?;
        Ok(vec![txt, srt])
    })();
    let _ = std::fs::remove_file(&wav_path);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipping_times_keep_global_offsets_and_labels_in_documents() {
        let turn = speakers::Turn {
            start: 10.0,
            end: 12.0,
            speakers: vec![2],
        };
        let cues = read_cues(
            r#"{"transcription":[{"text":"最后一句","offsets":{"from":500,"to":1900}}]}"#,
            &turn,
        )
        .unwrap();
        assert_eq!(cues[0].start, 10.5);
        let (text, srt) = speaker_documents(&cues, 2);
        assert!(text.contains("说话人 2") && text.contains("最后一句"));
        assert!(srt.contains("00:00:10,500 --> 00:00:11,900\n[说话人 2] 最后一句"));
        assert!(read_cues(r#"{"transcription":[{"text":"无时间"}]}"#, &turn).is_err());
    }
    #[test]
    #[ignore = "需要本地识别组件和四人音频样例"]
    fn local_four_speakers_end_to_end() {
        crate::bili::configure_tools_path();
        let source = Path::new("tmp/diarization-download/0-four-speakers-zh.wav");
        let directory = Path::new("output/speakers");
        let paths = run(
            source,
            &find_default_model().unwrap(),
            "zh",
            directory,
            SpeakerOptions {
                enabled: true,
                count: 0,
            },
            &|s| eprintln!("{s}"),
        )
        .unwrap();
        let text = std::fs::read_to_string(&paths[0]).unwrap();
        for number in 1..=4 {
            assert!(text.contains(&format!("说话人 {number}")));
        }
        assert!(!text.contains("说话人 5"));
        assert!(std::fs::read_to_string(&paths[1])
            .unwrap()
            .contains("[说话人 4]"));
    }
    #[test]
    #[ignore = "需要网络，验证B站独立音轨下载"]
    fn bili_download_is_an_audio_track_without_video() {
        crate::bili::configure_tools_path();
        let client = crate::bili::Client::new();
        let (video, page) = client
            .fetch_video("https://www.bilibili.com/video/BV1tG4y147F9/")
            .unwrap();
        let streams = client.fetch_streams(&video, page).unwrap();
        let url = streams
            .iter()
            .find_map(|stream| stream.audio_url.as_deref())
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("audio.m4a");
        client.download_to_file(url, &path, &|_| {}).unwrap();
        let result = command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(result.status.success());
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let streams = value["streams"].as_array().unwrap();
        assert!(
            !streams.is_empty() && streams.iter().all(|stream| stream["codec_type"] == "audio")
        );
    }
}
