use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Track {
    pub language: String,
    pub label: String,
    pub automatic: bool,
}
#[derive(Clone, Debug)]
pub struct Video {
    pub title: String,
    pub url: String,
    pub tracks: Vec<Track>,
}

pub fn link(input: &str) -> Option<String> {
    input.split_whitespace().find_map(|part| {
        let start = part.find("https://").or_else(|| part.find("http://"))?;
        let value = part[start..].trim_end_matches(['。', '，', ')', ']', ',', '.']);
        let url = reqwest::Url::parse(value).ok()?;
        let host = url.host_str()?;
        if !(host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com")) {
            return None;
        }
        let id = if host == "youtu.be" {
            url.path_segments()?.next()?.to_owned()
        } else if url.path() == "/watch" {
            url.query_pairs().find(|(k, _)| k == "v")?.1.into_owned()
        } else {
            let parts: Vec<_> = url.path_segments()?.collect();
            if parts.len() < 2 || !matches!(parts[0], "shorts" | "live" | "embed") {
                return None;
            }
            parts[1].to_owned()
        };
        if id.len() != 11
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        Some(format!("https://www.youtube.com/watch?v={id}"))
    })
}

pub fn metadata(value: &Value, url: String) -> Video {
    let mut tracks = Vec::new();
    for (key, automatic) in [("subtitles", false), ("automatic_captions", true)] {
        if let Some(languages) = value[key].as_object() {
            for (language, formats) in languages {
                if language == "live_chat"
                    || !language
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-')
                {
                    continue;
                }
                if let Some(format) = formats
                    .as_array()
                    .and_then(|a| a.iter().find(|f| f["ext"] == "json3"))
                {
                    let name = format["name"].as_str().unwrap_or(language);
                    tracks.push(Track {
                        language: language.clone(),
                        label: format!(
                            "{name} ({language}) · {}",
                            if automatic {
                                "自动字幕"
                            } else {
                                "作者字幕"
                            }
                        ),
                        automatic,
                    });
                }
            }
        }
    }
    tracks.sort_by_key(|t| {
        (
            if t.language.starts_with("zh") {
                0
            } else if t.language.starts_with("en") {
                1
            } else {
                2
            },
            t.automatic,
            t.language.clone(),
        )
    });
    Video {
        title: value["title"].as_str().unwrap_or("YouTube 视频").into(),
        url,
        tracks,
    }
}

pub fn cookie_path() -> PathBuf {
    crate::zhihu_login::session_directory().join("youtube-cookies.txt")
}
pub fn import_cookies(path: &Path) -> Result<()> {
    if std::fs::metadata(path)?.len() > 2_000_000 {
        bail!("登录状态文件过大");
    }
    let text = std::fs::read_to_string(path)?;
    let content = cookie_contents(&text)?;
    let output = cookie_path();
    std::fs::create_dir_all(output.parent().unwrap())?;
    std::fs::write(output, content)?;
    Ok(())
}
fn cookie_contents(text: &str) -> Result<String> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let row = line.trim_start_matches("#HttpOnly_");
        if row.starts_with('#') || row.trim().is_empty() {
            continue;
        }
        let fields: Vec<_> = row.split('\t').collect();
        if fields.len() != 7 {
            continue;
        }
        let domain = fields[0].trim_start_matches('.');
        if domain == "youtube.com" || domain.ends_with(".youtube.com") {
            entries.push(line.to_owned());
        }
    }
    if entries.is_empty() {
        bail!("没有找到 YouTube 登录状态，请选择 Netscape 格式的 cookies.txt 文件");
    }
    Ok(format!(
        "# Netscape HTTP Cookie File\n{}\n",
        entries.join("\n")
    ))
}

fn error_message(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    if text.contains("not a bot") || text.contains("Sign in") {
        "YouTube 要求登录验证，请使用右上角「导入登录状态」后重新获取。".into()
    } else {
        format!("YouTube 获取失败：{text}")
    }
}

pub fn probe(input: &str, tool: &Path) -> Result<Video> {
    let url = link(input).context("请粘贴 YouTube 视频、Shorts 或短链接")?;
    let mut command = std::process::Command::new(tool);
    crate::external::configure_command(&mut command, &url);
    let output = command
        .args([
            "--ignore-config",
            "--dump-single-json",
            "--no-playlist",
            "--socket-timeout",
            "20",
            "--retries",
            "1",
        ])
        .arg(&url)
        .output()?;
    if !output.status.success() {
        bail!("{}", error_message(&output.stderr));
    }
    Ok(metadata(&serde_json::from_slice(&output.stdout)?, url))
}

pub fn lines(value: &Value) -> Result<Vec<crate::bili::SubLine>> {
    let mut lines: Vec<crate::bili::SubLine> = Vec::new();
    let events = value["events"]
        .as_array()
        .context("字幕内容不是有效的 JSON3")?;
    for event in events {
        let Some(segments) = event["segs"].as_array() else {
            continue;
        };
        let text: String = segments.iter().filter_map(|s| s["utf8"].as_str()).collect();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let from = event["tStartMs"].as_f64().unwrap_or(0.0) / 1000.0;
        let to = from + event["dDurationMs"].as_f64().unwrap_or(1000.0) / 1000.0;
        if let Some(previous) = lines.last_mut() {
            if previous.content == text && from <= previous.to {
                previous.to = previous.to.max(to);
                continue;
            }
        }
        lines.push(crate::bili::SubLine {
            from,
            to,
            content: text.into(),
        });
    }
    if lines.is_empty() {
        bail!("字幕为空，未生成导出文件");
    }
    Ok(lines)
}

pub fn export(
    video: &Video,
    track: &Track,
    tool: &Path,
    directory: &Path,
    format: crate::export::TextFormat,
) -> Result<Vec<PathBuf>> {
    let temp = tempfile::tempdir()?;
    let mut command = std::process::Command::new(tool);
    crate::external::configure_command(&mut command, &video.url);
    let output = command
        .args([
            "--ignore-config",
            "--skip-download",
            "--no-playlist",
            "--socket-timeout",
            "20",
            "--retries",
            "1",
            "--sub-format",
            "json3",
            "--sub-langs",
        ])
        .arg(format!("^{}$", track.language))
        .arg(if track.automatic {
            "--write-auto-subs"
        } else {
            "--write-subs"
        })
        .arg(if track.automatic {
            "--no-write-subs"
        } else {
            "--no-write-auto-subs"
        })
        .arg("-P")
        .arg(temp.path())
        .args(["-o", "captions.%(ext)s"])
        .arg(&video.url)
        .output()?;
    if !output.status.success() {
        bail!("{}", error_message(&output.stderr));
    }
    let file = temp
        .path()
        .join(format!("captions.{}.json3", track.language));
    let content =
        std::fs::read(&file).context("该语言字幕暂时无法下载，请重新获取或选择其他语言")?;
    let body = lines(&serde_json::from_slice(&content)?)?;
    std::fs::create_dir_all(directory)?;
    let stem = format!(
        "{}_{}{}",
        crate::bili::sanitize_filename(&video.title),
        track.language,
        if track.automatic { "_自动字幕" } else { "" }
    );
    let document = directory.join(format!("{stem}.{}", format.extension()));
    crate::export::save(
        &document,
        &video.title,
        &format!(
            "{}\n\n来源：{}",
            crate::bili::lines_to_txt(&body),
            video.url
        ),
        format,
    )?;
    let srt = directory.join(format!("{stem}.srt"));
    std::fs::write(&srt, crate::bili::lines_to_srt(&body))?;
    Ok(vec![document, srt])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cookie_import_keeps_only_youtube_and_explains_bot_checks() {
        let value = "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\tSID\ttest\n#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t0\tHSID\ttest\n.evil.test\tTRUE\t/\tTRUE\t0\tOTHER\tprivate\n.youtube.com.evil.test\tTRUE\t/\tTRUE\t0\tFAKE\tprivate";
        let content = cookie_contents(value).unwrap();
        assert!(content.contains("#HttpOnly_"));
        assert!(!content.contains("private"));
        assert!(cookie_contents("invalid").is_err());
        assert!(error_message(b"Sign in to confirm you're not a bot").contains("导入登录状态"));
    }
    #[test]
    fn links_select_one_video_and_reject_other_hosts() {
        assert_eq!(
            link("分享 https://youtu.be/BaW_jenozKc?si=abc"),
            Some("https://www.youtube.com/watch?v=BaW_jenozKc".into())
        );
        assert!(link("https://www.youtube.com/shorts/BaW_jenozKc").is_some());
        assert!(link("https://youtube.com.evil.test/watch?v=BaW_jenozKc").is_none());
        assert!(link("https://www.youtube.com/playlist?list=123").is_none());
    }
    #[test]
    fn manual_and_auto_tracks_stay_distinct_and_last_cue_is_kept() {
        let value = serde_json::json!({"title":"示例", "subtitles":{"en":[{"ext":"json3","name":"English"}]},"automatic_captions":{"en":[{"ext":"json3"}],"live_chat":[{"ext":"json3"}]}});
        let video = metadata(&value, "https://www.youtube.com/watch?v=BaW_jenozKc".into());
        assert_eq!(video.tracks.len(), 2);
        assert!(!video.tracks[0].automatic);
        assert!(video.tracks[1].automatic);
        let events = serde_json::json!({"events":[{"tStartMs":0,"dDurationMs":2000,"segs":[{"utf8":"Hello "},{"utf8":"world"}]},{"tStartMs":1000,"dDurationMs":2000,"segs":[{"utf8":"Hello world"}]},{"tStartMs":4000,"dDurationMs":1000,"segs":[{"utf8":"最后一行"}]}]});
        let cues = lines(&events).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].to, 3.0);
        assert!(crate::bili::lines_to_srt(&cues).contains("最后一行"));
    }
}
