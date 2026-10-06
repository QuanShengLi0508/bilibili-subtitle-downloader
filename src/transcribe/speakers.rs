use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub enabled: bool,
    /// Zero means automatic speaker counting.
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speakers: Vec<usize>,
}
impl Turn {
    pub fn label(&self) -> String {
        if self.speakers.len() == 1 {
            format!("说话人 {}", self.speakers[0])
        } else {
            format!(
                "重叠说话（{}）",
                self.speakers
                    .iter()
                    .map(|n| format!("说话人 {n}"))
                    .collect::<Vec<_>>()
                    .join(" / ")
            )
        }
    }
}

pub fn resources() -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            bases.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    bases
        .into_iter()
        .map(|base| base.join("diarization"))
        .find(|dir| {
            [
                "sherpa-onnx-offline-speaker-diarization.exe",
                "segmentation.onnx",
                "embedding.onnx",
            ]
            .iter()
            .all(|file| dir.join(file).is_file())
        })
}

pub fn parse(output: &str) -> Result<Vec<Turn>> {
    let mut raw = Vec::new();
    for line in output.lines() {
        let columns: Vec<_> = line.split_whitespace().collect();
        if columns.len() < 4 || columns[1] != "--" {
            continue;
        }
        let Some(speaker) = columns[3].strip_prefix("speaker_") else {
            continue;
        };
        let start: f64 = columns[0].parse().context("说话时间无法识别")?;
        let end: f64 = columns[2].parse().context("说话时间无法识别")?;
        let speaker: usize = speaker.parse().context("说话人编号无法识别")?;
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            bail!("说话时间异常，请重新识别");
        }
        raw.push((start, end, speaker));
    }
    if raw.is_empty() {
        bail!("未检测到可识别的人声，未生成多人转写文件");
    }
    raw.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    let mut labels = HashMap::new();
    for (_, _, speaker) in &raw {
        let next = labels.len() + 1;
        labels.entry(*speaker).or_insert(next);
    }
    let mut edges: Vec<_> = raw
        .iter()
        .flat_map(|(start, end, _)| [*start, *end])
        .collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    let mut turns: Vec<Turn> = Vec::new();
    for pair in edges.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let mut speakers: Vec<_> = raw
            .iter()
            .filter(|(a, b, _)| *a < end && *b > start)
            .map(|(_, _, id)| labels[id])
            .collect();
        speakers.sort_unstable();
        speakers.dedup();
        if speakers.is_empty() {
            continue;
        }
        if let Some(last) = turns.last_mut() {
            if last.speakers == speakers && start - last.end <= 0.35 {
                last.end = end;
                continue;
            }
        }
        turns.push(Turn {
            start,
            end,
            speakers,
        });
    }
    Ok(turns)
}

pub fn detect(wav: &Path, options: Options) -> Result<Vec<Turn>> {
    if options.count > 12 {
        bail!("说话人数支持自动或 1–12 人");
    }
    let dir = resources().context("多人识别资源缺失，请重新安装新版拾文")?;
    let wav = wav.canonicalize()?;
    let mut command = super::command(&dir.join("sherpa-onnx-offline-speaker-diarization.exe"));
    // Relative model names also work with Chinese Windows installation paths.
    command.current_dir(&dir).args([
        "--segmentation.pyannote-model=segmentation.onnx",
        "--embedding.model=embedding.onnx",
        "--segmentation.num-threads=2",
        "--embedding.num-threads=2",
        "--clustering.cluster-threshold=0.9",
    ]);
    if options.count > 0 {
        command.arg(format!("--clustering.num-clusters={}", options.count));
    }
    let result = command.arg(wav).output().context("启动多人声音识别失败")?;
    if !result.status.success() {
        bail!(
            "多人声音识别失败：{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    parse(&String::from_utf8_lossy(&result.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_roles_reappear_and_overlap_is_not_assigned_to_one_person() {
        let turns = parse(
            "Started\n0 -- 2 speaker_07\n2 -- 4 speaker_02\n4 -- 7 speaker_07\n5 -- 6 speaker_02",
        )
        .unwrap();
        assert_eq!(
            turns.iter().map(|t| t.speakers.clone()).collect::<Vec<_>>(),
            vec![vec![1], vec![2], vec![1], vec![1, 2], vec![1]]
        );
        assert!(turns[3].label().contains("重叠"));
        assert_eq!(turns.last().unwrap().end, 7.0);
        assert!(parse("No speech").is_err());
        assert!(parse("NaN -- 1 speaker_00").is_err());
    }
}
