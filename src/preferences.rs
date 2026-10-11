//! Local UI preferences only. Draft links, source content, cookies and login
//! tokens deliberately have no representation in this schema.
use crate::export::TextFormat;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u64 = 1;
const MAX_CONFIG_BYTES: u64 = 128 * 1024;
const MAX_RECENT_FILES: usize = 10;

/// `platform` is a stable identifier; the GUI maps it to its current modes.
/// Recent paths are most-recent first and may include exported image folders.
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub platform: String,
    pub text_format: TextFormat,
    pub output_dir: PathBuf,
    pub transcribe_language: String,
    pub speaker_diarization: bool,
    pub speaker_count: usize,
    pub recent_files: Vec<PathBuf>,
}

impl Preferences {
    pub fn new(default_output: PathBuf) -> Self {
        Self {
            platform: "bilibili".into(),
            text_format: TextFormat::Txt,
            output_dir: default_output,
            transcribe_language: "auto".into(),
            speaker_diarization: true,
            speaker_count: 0,
            recent_files: Vec::new(),
        }
    }

    /// A missing, unreadable, oversized or malformed configuration never stops
    /// startup. Valid individual fields survive other invalid field values.
    pub fn load(default_output: &Path) -> Self {
        Self::load_from(&config_path(), default_output)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&config_path())
    }

    /// Adds a completed operation's output paths in the order supplied by the
    /// caller. Passing `[]` just removes stale entries and duplicates.
    pub fn remember_files(&mut self, paths: &[PathBuf]) {
        self.recent_files = clean_recent(paths.iter().chain(self.recent_files.iter()));
    }

    fn load_from(path: &Path, default_output: &Path) -> Self {
        let defaults = Self::new(default_output.to_owned());
        let Ok(file) = std::fs::File::open(path) else {
            return defaults;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_CONFIG_BYTES
        {
            return defaults;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return defaults;
        };
        if value["version"].as_u64() != Some(SCHEMA_VERSION) || !value.is_object() {
            return defaults;
        }
        let mut preferences = defaults;
        if let Some(platform) = value["platform"]
            .as_str()
            .filter(|value| valid_platform(value))
        {
            preferences.platform = platform.into();
        }
        preferences.text_format = match value["text_format"].as_str() {
            Some("md") => TextFormat::Markdown,
            Some("docx") => TextFormat::Word,
            Some("pdf") => TextFormat::Pdf,
            _ => TextFormat::Txt,
        };
        if let Some(path) = value["output_dir"].as_str().map(PathBuf::from) {
            // A folder may have been temporarily disconnected or deleted. Keep
            // a valid absolute location so an export can recreate it later.
            if valid_path(&path) && !path.is_file() {
                preferences.output_dir = path;
            }
        }
        if let Some(language) = value["transcribe_language"]
            .as_str()
            .filter(|value| valid_language(value))
        {
            preferences.transcribe_language = language.into();
        }
        if let Some(enabled) = value["speaker_diarization"].as_bool() {
            preferences.speaker_diarization = enabled;
        }
        if let Some(count) = value["speaker_count"].as_u64().filter(|count| *count <= 12) {
            preferences.speaker_count = count as usize;
        }
        if let Some(entries) = value["recent_files"].as_array() {
            let paths: Vec<_> = entries
                .iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect();
            preferences.recent_files = clean_recent(paths.iter());
        }
        preferences
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        if !valid_path(&self.output_dir) || self.output_dir.is_file() {
            bail!("输出目录无效，未保存设置");
        }
        let recent_files = clean_recent(self.recent_files.iter());
        // Build the whitelist explicitly: unknown fields from a previous file
        // are never carried over, including any pasted URLs or credentials.
        let value = json!({
            "version": SCHEMA_VERSION,
            "platform": if valid_platform(&self.platform) { self.platform.as_str() } else { "bilibili" },
            "text_format": self.text_format.extension(),
            "output_dir": self.output_dir.to_str().context("输出目录名称无法保存")?,
            "transcribe_language": if valid_language(&self.transcribe_language) { self.transcribe_language.as_str() } else { "auto" },
            "speaker_diarization": self.speaker_diarization,
            "speaker_count": if self.speaker_count <= 12 { self.speaker_count } else { 0 },
            "recent_files": recent_files.iter().filter_map(|path| path.to_str()).collect::<Vec<_>>(),
        });
        let mut bytes = serde_json::to_vec_pretty(&value)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            bail!("设置内容过大，未保存");
        }
        let directory = path.parent().context("设置目录无效")?;
        std::fs::create_dir_all(directory).context("无法创建设置目录")?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".preferences-")
            .suffix(".tmp")
            .tempfile_in(directory)
            .context("无法写入设置目录")?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        // Only the settings file is replaced; user exports are never opened
        // for writing or renamed by preferences/history maintenance.
        temporary
            .persist(path)
            .context("保存设置失败，原设置已保留")?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    crate::zhihu_login::session_directory().join("preferences.json")
}

fn valid_platform(platform: &str) -> bool {
    matches!(
        platform,
        "bilibili"
            | "douyin"
            | "xhs"
            | "youtube"
            | "zhihu"
            | "wechat_article"
            | "wechat_channels"
            | "twitter"
            | "transcribe"
    )
}

fn valid_language(language: &str) -> bool {
    matches!(language, "auto" | "zh" | "en")
}

fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .to_str()
            .is_some_and(|value| !value.contains('\0') && value.len() <= 32_000)
}

fn clean_recent<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut recent = Vec::new();
    for path in paths {
        if !valid_path(path) || !(path.is_file() || path.is_dir()) {
            continue;
        }
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        #[cfg(windows)]
        let key = canonical
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        #[cfg(not(windows))]
        let key = canonical;
        if seen.insert(key) {
            recent.push(path.clone());
            if recent.len() == MAX_RECENT_FILES {
                break;
            }
        }
    }
    recent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_corrupt_and_future_configs_use_defaults_without_touching_input() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("settings.json");
        let output = directory.path().join("导出");
        let defaults = Preferences::new(output.clone());
        assert_eq!(Preferences::load_from(&config, &output), defaults);
        for bytes in [
            b"not-json".as_slice(),
            br#"{"version":99,"platform":"youtube"}"#,
        ] {
            std::fs::write(&config, bytes).unwrap();
            assert_eq!(Preferences::load_from(&config, &output), defaults);
            assert_eq!(std::fs::read(&config).unwrap(), bytes);
        }
        std::fs::write(&config, vec![b' '; MAX_CONFIG_BYTES as usize + 1]).unwrap();
        assert_eq!(Preferences::load_from(&config, &output), defaults);
    }

    #[test]
    fn preferences_round_trip_atomically_without_source_links_or_export_changes() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("中文导出目录");
        std::fs::create_dir(&output).unwrap();
        let exported = output.join("已保存的正文.md");
        std::fs::write(&exported, "原有正文保持完整").unwrap();
        let config = directory.path().join("settings.json");
        let mut preferences = Preferences::new(output.clone());
        preferences.platform = "wechat_article".into();
        preferences.text_format = TextFormat::Markdown;
        preferences.transcribe_language = "zh".into();
        preferences.speaker_diarization = false;
        preferences.speaker_count = 3;
        preferences.remember_files(&[exported.clone()]);
        preferences.save_to(&config).unwrap();
        assert_eq!(
            Preferences::load_from(&config, directory.path()),
            preferences
        );
        preferences.text_format = TextFormat::Pdf;
        preferences.save_to(&config).unwrap();
        assert_eq!(
            Preferences::load_from(&config, directory.path()),
            preferences
        );
        assert_eq!(
            std::fs::read_to_string(&exported).unwrap(),
            "原有正文保持完整"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
        let saved: Value = serde_json::from_slice(&std::fs::read(config).unwrap()).unwrap();
        assert_eq!(saved.as_object().unwrap().len(), 8);
        assert!(saved.get("link").is_none() && saved.get("cookies").is_none());
    }

    #[test]
    fn invalid_fields_fall_back_but_valid_settings_survive() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("默认");
        let config = directory.path().join("settings.json");
        std::fs::write(&config, json!({"version":1, "platform":"https://private.test/?token=secret", "text_format":"pdf", "output_dir":"../wrong", "speaker_count":999, "speaker_diarization":false, "transcribe_language":"--command", "link":"https://private.test", "cookies":"secret"}).to_string()).unwrap();
        let loaded = Preferences::load_from(&config, &output);
        assert_eq!(loaded.platform, "bilibili");
        assert_eq!(loaded.output_dir, output);
        assert_eq!(loaded.text_format, TextFormat::Pdf);
        assert_eq!(loaded.transcribe_language, "auto");
        assert!(!loaded.speaker_diarization);
        assert_eq!(loaded.speaker_count, 0);
        loaded.save_to(&config).unwrap();
        let saved = std::fs::read_to_string(config).unwrap();
        assert!(!saved.contains("secret") && !saved.contains("private.test"));
    }

    #[test]
    fn recent_history_is_mru_deduplicated_bounded_and_keeps_gallery_folders() {
        let directory = tempfile::tempdir().unwrap();
        let mut preferences = Preferences::new(directory.path().to_owned());
        let paths: Vec<_> = (0..12)
            .map(|number| {
                let path = directory.path().join(format!("结果{number}.txt"));
                std::fs::write(&path, "正文").unwrap();
                path
            })
            .collect();
        preferences.remember_files(&paths);
        assert_eq!(preferences.recent_files, paths[..10]);
        let duplicate = directory.path().join(".").join("结果5.txt");
        preferences.remember_files(&[
            paths[5].clone(),
            duplicate,
            directory.path().join("missing.txt"),
        ]);
        assert_eq!(preferences.recent_files[0], paths[5]);
        assert_eq!(preferences.recent_files.len(), 10);
        assert_eq!(
            preferences
                .recent_files
                .iter()
                .filter(|path| **path == paths[5])
                .count(),
            1
        );
        let gallery = directory.path().join("图文文件夹");
        std::fs::create_dir(&gallery).unwrap();
        preferences.remember_files(&[gallery.clone()]);
        assert_eq!(preferences.recent_files[0], gallery);
        std::fs::remove_file(&paths[5]).unwrap();
        preferences.remember_files(&[]);
        assert!(!preferences.recent_files.contains(&paths[5]));
        assert_eq!(preferences.recent_files.len(), 9);
    }

    #[test]
    fn invalid_output_cannot_replace_settings_or_an_export() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("settings.json");
        let mut preferences = Preferences::new(directory.path().to_owned());
        preferences.save_to(&config).unwrap();
        let original = std::fs::read(&config).unwrap();
        preferences.output_dir = config.clone();
        assert!(preferences.save_to(&config).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), original);
    }
}
