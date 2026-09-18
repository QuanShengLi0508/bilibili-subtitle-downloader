use anyhow::{anyhow, bail, Context, Result};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn cookie_file() -> PathBuf {
    let base = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join(".bili-subtitle-cookies.json")
}

fn load_cookie_store() -> Arc<CookieStoreMutex> {
    let store: cookie_store::CookieStore = std::fs::read_to_string(cookie_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    Arc::new(CookieStoreMutex::new(store))
}

pub struct VideoInfo {
    pub aid: i64,
    pub bvid: String,
    pub title: String,
    pub pages: Vec<PageInfo>,
}

#[derive(Clone)]
pub struct PageInfo {
    pub page: usize,
    pub part: String,
    pub cid: i64,
}

pub struct SubTrack {
    pub lan: String,
    pub lan_doc: String,
    pub url: String,
}

pub struct SubLine {
    pub from: f64,
    pub to: f64,
    pub content: String,
}

#[derive(Clone)]
pub struct VideoStream {
    pub quality_id: u64,
    pub label: String,
    pub video_url: String,
    pub audio_url: Option<String>,
}

pub struct QrLogin {
    pub qrcode_key: String,
    pub url: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum QrPoll {
    Waiting,
    Scanned,
    Confirmed,
    Expired,
}

pub struct Client {
    http: reqwest::blocking::Client,
    store: Arc<CookieStoreMutex>,
}

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// wbi 签名用的混淆表，来自社区公开文档
const MIXIN_TABLE: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

impl Client {
    pub fn new() -> Self {
        let store = load_cookie_store();
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .cookie_provider(store.clone())
            .referer(true)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("构建 HTTP 客户端失败");
        Self { http, store }
    }

    pub fn has_saved_cookies() -> bool {
        cookie_file().exists()
    }

    pub fn clear_saved_cookies() {
        let _ = std::fs::remove_file(cookie_file());
    }

    pub fn save_cookies(&self) -> Result<()> {
        let s = serde_json::to_string(&*self.store.lock().unwrap())?;
        std::fs::write(cookie_file(), s)?;
        Ok(())
    }

    pub fn get_json(&self, url: &str) -> Result<Value> {
        let resp = self
            .http
            .get(url)
            .header("Referer", "https://www.bilibili.com/")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .header("Origin", "https://www.bilibili.com")
            .send()
            .with_context(|| format!("请求失败: {url}"))?;
        let status = resp.status();
        let text = resp.text()?;
        if !status.is_success() {
            bail!("HTTP {status}");
        }
        let v: Value =
            serde_json::from_str(&text).with_context(|| format!("返回内容不是JSON: {url}"))?;
        Ok(v)
    }

    /// 支持完整链接、短链接 b23.tv、纯 BV 号 / AV 号
    pub fn resolve_input(&self, input: &str) -> Result<String> {
        let input = input.trim();
        if input.is_empty() {
            bail!("请输入B站视频链接");
        }
        let lower = input.to_ascii_lowercase();
        if lower.contains("b23.tv")
            || lower.starts_with("http")
                && extract_bvid(input).is_none()
                && extract_aid(input).is_none()
        {
            // 短链接：跟随重定向拿到最终地址
            let resp = self
                .http
                .get(input)
                .header("Referer", "https://www.bilibili.com/")
                .send()?;
            let final_url = resp.url().to_string();
            if extract_bvid(&final_url).is_none() && extract_aid(&final_url).is_none() {
                bail!("无法从链接中识别视频ID: {final_url}");
            }
            return Ok(final_url);
        }
        Ok(input.to_string())
    }

    pub fn fetch_video(&self, input: &str) -> Result<(VideoInfo, usize)> {
        // 先访问首页拿到 buvid 等 Cookie，避免接口返回 412
        let _ = self
            .http
            .get("https://www.bilibili.com/")
            .header("Referer", "https://www.bilibili.com/")
            .send();

        let url_part = self.resolve_input(input)?;
        let (bvid, aid) = match (extract_bvid(&url_part), extract_aid(&url_part)) {
            (Some(bv), _) => (Some(bv), None),
            (None, Some(av)) => (None, Some(av)),
            _ => bail!("无法从输入中识别 BV 号或 AV 号"),
        };
        let page = extract_page(&url_part).unwrap_or(1);

        let api = match (&bvid, &aid) {
            (Some(bv), _) => format!("https://api.bilibili.com/x/web-interface/view?bvid={bv}"),
            (None, Some(a)) => format!("https://api.bilibili.com/x/web-interface/view?aid={a}"),
            _ => unreachable!(),
        };
        let v = self.get_json(&api)?;
        if v["code"].as_i64() != Some(0) {
            bail!(
                "获取视频信息失败: {} ({})",
                v["message"],
                v["code"].as_i64().unwrap_or(-1)
            );
        }
        let data = &v["data"];
        let info = VideoInfo {
            aid: data["aid"].as_i64().unwrap_or(0),
            bvid: data["bvid"]
                .as_str()
                .unwrap_or(bvid.as_deref().unwrap_or(""))
                .to_string(),
            title: data["title"].as_str().unwrap_or("未知标题").to_string(),
            pages: data["pages"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|p| PageInfo {
                    page: p["page"].as_u64().unwrap_or(1) as usize,
                    part: p["part"].as_str().unwrap_or("").to_string(),
                    cid: p["cid"].as_i64().unwrap_or(0),
                })
                .collect(),
        };
        if info.pages.is_empty() {
            bail!("视频信息里没有分P数据");
        }
        Ok((info, page))
    }

    pub fn fetch_tracks(&self, video: &VideoInfo, page: usize) -> Result<Vec<SubTrack>> {
        let cid = video
            .pages
            .iter()
            .find(|p| p.page == page)
            .map(|p| p.cid)
            .or_else(|| video.pages.first().map(|p| p.cid))
            .ok_or_else(|| anyhow!("找不到分P {page} 的 cid"))?;

        let query = self.wbi_signed_query(vec![
            ("aid".to_string(), video.aid.to_string()),
            ("cid".to_string(), cid.to_string()),
            ("bvid".to_string(), video.bvid.clone()),
        ])?;
        let api = format!("https://api.bilibili.com/x/player/wbi/v2?{query}");
        let v = self.get_json(&api)?;
        if v["code"].as_i64() != Some(0) {
            bail!(
                "获取字幕列表失败: {} ({})",
                v["message"],
                v["code"].as_i64().unwrap_or(-1)
            );
        }
        let subs = &v["data"]["subtitle"]["subtitles"];
        let mut tracks = Vec::new();
        if let Some(arr) = subs.as_array() {
            for s in arr {
                let url = s["subtitle_url"].as_str().unwrap_or("").to_string();
                if url.is_empty() {
                    continue;
                }
                let url = if let Some(rest) = url.strip_prefix("//") {
                    format!("https://{rest}")
                } else {
                    url
                };
                tracks.push(SubTrack {
                    lan: s["lan"].as_str().unwrap_or("unknown").to_string(),
                    lan_doc: s["lan_doc"].as_str().unwrap_or("未知语言").to_string(),
                    url,
                });
            }
        }
        Ok(tracks)
    }

    pub fn qr_generate(&self) -> Result<QrLogin> {
        let v =
            self.get_json("https://passport.bilibili.com/x/passport-login/web/qrcode/generate")?;
        if v["code"].as_i64() != Some(0) {
            bail!(
                "获取登录二维码失败: {}",
                v["message"].as_str().unwrap_or("未知错误")
            );
        }
        Ok(QrLogin {
            qrcode_key: v["data"]["qrcode_key"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            url: v["data"]["url"].as_str().unwrap_or_default().to_string(),
        })
    }

    pub fn qr_poll(&self, key: &str) -> Result<QrPoll> {
        let api = format!(
            "https://passport.bilibili.com/x/passport-login/web/qrcode/poll?qrcode_key={key}"
        );
        let v = self.get_json(&api)?;
        // 新版接口外层 code 恒为 0，真实状态在 data.code 里（86101未扫码/86090已扫/86038过期/0成功）
        let status = v["data"]["code"].as_i64().or_else(|| v["code"].as_i64());
        match status {
            Some(0) => {
                self.save_cookies()?;
                Ok(QrPoll::Confirmed)
            }
            Some(86038) => Ok(QrPoll::Expired),
            Some(86090) => Ok(QrPoll::Scanned),
            _ => Ok(QrPoll::Waiting),
        }
    }

    pub fn debug_tracks_json(&self, video: &VideoInfo, page: usize) -> Result<String> {
        let cid = video
            .pages
            .iter()
            .find(|p| p.page == page)
            .map(|p| p.cid)
            .unwrap_or(0);
        let query = self.wbi_signed_query(vec![
            ("aid".to_string(), video.aid.to_string()),
            ("cid".to_string(), cid.to_string()),
            ("bvid".to_string(), video.bvid.clone()),
        ])?;
        let api = format!("https://api.bilibili.com/x/player/wbi/v2?{query}");
        let v = self.get_json(&api)?;
        Ok(serde_json::to_string_pretty(&v)?)
    }

    pub fn fetch_subtitle_body(&self, url: &str) -> Result<Vec<SubLine>> {
        let v = self.get_json(url)?;
        let body = v["body"]
            .as_array()
            .ok_or_else(|| anyhow!("字幕文件格式异常"))?;
        let mut lines = Vec::new();
        for item in body {
            lines.push(SubLine {
                from: item["from"].as_f64().unwrap_or(0.0),
                to: item["to"].as_f64().unwrap_or(0.0),
                content: item["content"].as_str().unwrap_or("").to_string(),
            });
        }
        Ok(lines)
    }

    pub fn fetch_streams(&self, video: &VideoInfo, page: usize) -> Result<Vec<VideoStream>> {
        let cid = video
            .pages
            .iter()
            .find(|p| p.page == page)
            .map(|p| p.cid)
            .or_else(|| video.pages.first().map(|p| p.cid))
            .ok_or_else(|| anyhow!("找不到分P {page} 的 cid"))?;

        let query = self.wbi_signed_query(vec![
            ("bvid".to_string(), video.bvid.clone()),
            ("cid".to_string(), cid.to_string()),
            ("qn".to_string(), "0".to_string()),
            ("fnval".to_string(), "4048".to_string()),
            ("fourk".to_string(), "1".to_string()),
        ])?;
        let api = format!("https://api.bilibili.com/x/player/wbi/playurl?{query}");
        let v = self.get_json(&api)?;
        if v["code"].as_i64() != Some(0) {
            bail!(
                "获取播放地址失败: {} ({})",
                v["message"].as_str().unwrap_or(""),
                v["code"].as_i64().unwrap_or(-1)
            );
        }
        let data = &v["data"];
        let mut streams = Vec::new();

        if data["dash"]["video"].as_array().is_some() {
            let audio_url = data["dash"]["audio"]
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .max_by_key(|a| a["bandwidth"].as_u64().unwrap_or(0))
                        .and_then(|a| a["base_url"].as_str())
                })
                .map(|s| s.to_string());

            // 每个清晰度可能有多条编码（avc/hevc），优先 avc 兼容性更好
            let mut entries: Vec<(u64, String, bool)> = Vec::new();
            for item in data["dash"]["video"].as_array().unwrap_or(&Vec::new()) {
                let id = item["id"].as_u64().unwrap_or(0);
                let url = item["base_url"].as_str().unwrap_or("");
                if id == 0 || url.is_empty() {
                    continue;
                }
                let is_avc = item["codecs"].as_str().unwrap_or("").contains("avc");
                entries.push((id, url.to_string(), is_avc));
            }
            let mut seen: Vec<u64> = Vec::new();
            for (id, _, _) in &entries {
                if seen.contains(id) {
                    continue;
                }
                seen.push(*id);
                let best = entries
                    .iter()
                    .filter(|(i, _, _)| i == id)
                    .max_by_key(|(_, _, avc)| *avc as u8)
                    .unwrap();
                streams.push(VideoStream {
                    quality_id: *id,
                    label: quality_label(*id),
                    video_url: best.1.clone(),
                    audio_url: audio_url.clone(),
                });
            }
            streams.sort_by(|a, b| b.quality_id.cmp(&a.quality_id));
        } else if let Some(first) = data["durl"].as_array().and_then(|a| a.first()) {
            if let Some(url) = first["url"].as_str() {
                streams.push(VideoStream {
                    quality_id: 0,
                    label: "默认画质".to_string(),
                    video_url: url.to_string(),
                    audio_url: None,
                });
            }
        }

        if streams.is_empty() {
            bail!("没有可用的视频流");
        }
        Ok(streams)
    }

    pub fn ffmpeg_available() -> bool {
        std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn download_to_file(
        &self,
        url: &str,
        path: &std::path::Path,
        progress: &dyn Fn(f64),
    ) -> Result<()> {
        use std::io::Read;
        use std::io::Write;
        let mut resp = self
            .http
            .get(url)
            .header("Referer", "https://www.bilibili.com/")
            .header("Accept", "*/*")
            .send()?;
        if !resp.status().is_success() {
            bail!("下载失败: HTTP {}", resp.status());
        }
        let total = resp.content_length().unwrap_or(0) as f64;
        let mut file = std::fs::File::create(path)?;
        let mut downloaded: u64 = 0;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = resp.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            downloaded += n as u64;
            if total > 0.0 {
                progress(downloaded as f64 / total);
            }
        }
        file.flush()?;
        Ok(())
    }

    fn wbi_keys(&self) -> Result<(String, String)> {
        let v = self.get_json("https://api.bilibili.com/x/web-interface/nav")?;
        let img = v["data"]["wbi_img"]["img_url"]
            .as_str()
            .ok_or_else(|| anyhow!("获取 wbi img_url 失败"))?;
        let sub = v["data"]["wbi_img"]["sub_url"]
            .as_str()
            .ok_or_else(|| anyhow!("获取 wbi sub_url 失败"))?;
        Ok((file_key(img)?, file_key(sub)?))
    }

    fn wbi_signed_query(&self, mut params: Vec<(String, String)>) -> Result<String> {
        let (img_key, sub_key) = self.wbi_keys()?;
        let combined: String = format!("{img_key}{sub_key}");
        let mixin_key: String = MIXIN_TABLE
            .iter()
            .take(32)
            .filter_map(|&i| combined.chars().nth(i))
            .collect();

        let wts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        params.push(("wts".into(), wts.to_string()));
        params.sort();
        let query = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let digest = md5::compute(format!("{query}{mixin_key}"));
        let w_rid = format!("{digest:x}");
        Ok(format!("{query}&w_rid={w_rid}"))
    }
}

fn quality_label(id: u64) -> String {
    let names: &[(u64, &str)] = &[
        (127, "8K 超高清"),
        (126, "杜比视界"),
        (125, "HDR 真彩"),
        (120, "4K 超清"),
        (116, "1080P 60帧"),
        (112, "1080P 高码率"),
        (100, "智能修复"),
        (80, "1080P 高清"),
        (74, "720P 60帧"),
        (64, "720P 高清"),
        (32, "480P 清晰"),
        (16, "360P 流畅"),
    ];
    names
        .iter()
        .find(|(q, _)| *q == id)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| format!("清晰度 {id}"))
}

fn file_key(url: &str) -> Result<String> {
    let name = url.rsplit('/').next().unwrap_or("");
    let key = name.split('.').next().unwrap_or("");
    if key.is_empty() {
        bail!("wbi key 解析失败: {url}");
    }
    Ok(key.to_string())
}

fn extract_bvid(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let lower = s.to_ascii_lowercase();
    let hay = lower.as_bytes();
    let mut from = 0;
    while let Some(rel) = hay[from..].windows(2).position(|w| w == b"bv") {
        let start = from + rel + 2;
        let end = start + 10;
        if end <= bytes.len() && bytes[start..end].iter().all(|b| b.is_ascii_alphanumeric()) {
            return Some(s[start..end].to_string());
        }
        from = start;
    }
    None
}

fn extract_aid(s: &str) -> Option<i64> {
    let lower = s.to_ascii_lowercase();
    let hay = lower.as_bytes();
    let mut from = 0;
    while let Some(rel) = hay[from..].windows(2).position(|w| w == b"av") {
        let start = from + rel + 2;
        let ok_prefix = start == 0 || !hay[start - 1].is_ascii_alphanumeric();
        let digits: String = s[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if ok_prefix && !digits.is_empty() {
            return digits.parse().ok();
        }
        from = start;
    }
    None
}

fn extract_page(s: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    for prefix in ["?p=", "&p="] {
        if let Some(pos) = lower.find(prefix) {
            let start = pos + prefix.len();
            let digits: String = s[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(p) = digits.parse() {
                return Some(p);
            }
        }
    }
    None
}

pub fn format_srt_time(secs: f64) -> String {
    let total = (secs.max(0.0) * 1000.0).round() as u64;
    let ms = total % 1000;
    let s = (total / 1000) % 60;
    let m = (total / 60_000) % 60;
    let h = total / 3_600_000;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

pub fn lines_to_srt(lines: &[SubLine]) -> String {
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_srt_time(l.from),
            format_srt_time(l.to),
            l.content
        ));
    }
    out
}

pub fn lines_to_txt(lines: &[SubLine]) -> String {
    lines
        .iter()
        .map(|l| l.content.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

pub fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}
