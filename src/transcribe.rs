use crate::bili::sanitize_filename;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

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

pub fn run(media: &Path, model: &Path, language: &str, output_dir: &Path) -> Result<Vec<PathBuf>> {
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
    let out_base = output_dir.join(format!("{base_name}_转写"));
    let wav_path = output_dir.join(format!("{base_name}_转写.tmp.wav"));

    let convert = std::process::Command::new("ffmpeg")
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

    let result = std::process::Command::new(&cli)
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(&wav_path)
        .args(["-l", language, "-otxt", "-osrt"])
        .arg("-of")
        .arg(&out_base)
        .output()
        .context("启动 whisper-cli 失败")?;

    let _ = std::fs::remove_file(&wav_path);
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
    Ok(vec![txt_path, srt_path])
}
