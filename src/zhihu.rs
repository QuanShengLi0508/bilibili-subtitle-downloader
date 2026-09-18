use crate::bili::sanitize_filename;
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, REFERER, USER_AGENT};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
enum Target {
    Answer(String),
    Article(String),
}

fn cookie_file() -> PathBuf {
    let base = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(".zhihu-cookies.json")
}

pub fn has_saved_cookies() -> bool {
    cookie_file().exists()
}

pub fn save_cookie(raw: &str) -> Result<()> {
    let cookie = normalize_cookie(raw)?;
    if extract_cookie_value(&cookie, "d_c0").is_none() {
        bail!("Cookie 里没有找到 d_c0");
    }
    if extract_cookie_value(&cookie, "z_c0").is_none() {
        bail!("Cookie 里没有找到 z_c0，请确认已在浏览器登录知乎");
    }
    let value = serde_json::json!({ "cookie": cookie });
    std::fs::write(cookie_file(), serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

fn load_cookie() -> Result<String> {
    let text = std::fs::read_to_string(cookie_file()).context("读取知乎 Cookie 失败")?;
    let value: Value = serde_json::from_str(&text).context("知乎 Cookie 文件格式错误")?;
    let cookie = value
        .get("cookie")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if cookie.is_empty() {
        bail!("知乎 Cookie 为空，请重新保存");
    }
    Ok(cookie)
}

fn normalize_cookie(raw: &str) -> Result<String> {
    let value = raw.trim().strip_prefix("Cookie:").unwrap_or(raw.trim());
    let value = value.trim();
    if value.is_empty() {
        bail!("请先粘贴浏览器里的知乎 Cookie");
    }
    if !value.contains("d_c0=") || !value.contains("z_c0=") {
        bail!("Cookie 里必须包含 d_c0 和 z_c0");
    }
    Ok(value.to_string())
}

fn extract_cookie_value(cookie: &str, name: &str) -> Option<String> {
    cookie.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        if key.trim().eq_ignore_ascii_case(name) {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn parse_target(input: &str) -> Result<Target> {
    let input = input.trim();
    let normalized = if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let url = reqwest::Url::parse(&normalized).context("链接格式不正确")?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !(host == "zhihu.com"
        || host == "www.zhihu.com"
        || host == "zhuanlan.zhihu.com"
        || host.ends_with(".zhihu.com"))
    {
        bail!("请输入知乎回答或专栏链接");
    }

    let segments: Vec<String> = url
        .path_segments()
        .map(|parts| {
            parts
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    for index in 0..segments.len().saturating_sub(1) {
        match segments[index].as_str() {
            "answer" => {
                let id = &segments[index + 1];
                if id.chars().all(|c| c.is_ascii_digit()) {
                    return Ok(Target::Answer(id.clone()));
                }
            }
            "p" => {
                let id = &segments[index + 1];
                if id.chars().all(|c| c.is_ascii_digit()) {
                    return Ok(Target::Article(id.clone()));
                }
            }
            "article" => {
                let id = &segments[index + 1];
                if id.chars().all(|c| c.is_ascii_digit()) {
                    return Ok(Target::Article(id.clone()));
                }
            }
            _ => {}
        }
    }

    bail!("只支持知乎回答链接或专栏链接");
}

fn api_url(target: &Target) -> String {
    match target {
        Target::Answer(id) => format!("https://www.zhihu.com/api/v4/answers/{id}"),
        Target::Article(id) => format!("https://www.zhihu.com/api/v4/articles/{id}"),
    }
}

fn html_to_text(html: &str) -> Result<String> {
    let text = html2text::config::plain()
        .string_from_read(html.as_bytes(), 80)
        .context("正文 HTML 转文本失败")?;
    let text = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if text.is_empty() {
        bail!("知乎接口没有返回正文，可能登录状态已过期");
    }
    Ok(text)
}

fn pick_title(value: &Value, target: &Target) -> String {
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            value
                .pointer("/question/title")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
        })
        .map(str::trim)
        .map(sanitize_filename)
        .unwrap_or_default();

    if title.is_empty() {
        match target {
            Target::Answer(id) => format!("知乎回答_{id}"),
            Target::Article(id) => format!("知乎专栏_{id}"),
        }
    } else {
        title
    }
}

fn error_message(value: &Value, status: u16) -> String {
    let name = value
        .get("error")
        .and_then(|e| e.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = value
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !name.is_empty() || !message.is_empty() {
        if name == "need_login" {
            return "知乎要求登录，Cookie 可能已过期，请重新保存 Cookie".into();
        }
        if !message.is_empty() {
            return format!("知乎接口返回错误: {message}");
        }
        return format!("知乎接口返回错误: {name}");
    }
    format!("知乎接口请求失败（HTTP {status}）")
}

pub fn fetch_to_file(input: &str, output_dir: &std::path::Path) -> Result<PathBuf> {
    let target = parse_target(input)?;
    let cookie = load_cookie()?;
    let d_c0 =
        extract_cookie_value(&cookie, "d_c0").context("知乎 Cookie 缺少 d_c0，请重新保存")?;
    let page_url = match &target {
        Target::Answer(id) => format!("https://www.zhihu.com/question/0/answer/{id}"),
        Target::Article(id) => format!("https://zhuanlan.zhihu.com/p/{id}"),
    };
    let endpoint = api_url(&target);

    let signed = zhihu_sign::sign_zhihu_request(&endpoint, &d_c0, None);
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36"),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(REFERER, HeaderValue::from_str(&page_url)?);
    headers.insert(COOKIE, HeaderValue::from_str(&cookie)?);
    for (name, value) in signed {
        headers.insert(
            reqwest::header::HeaderName::from_bytes(name.as_bytes())?,
            HeaderValue::from_str(&value)?,
        );
    }

    let response = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(&endpoint)
        .headers(headers)
        .send()?;

    let status = response.status();
    let body: Value = response.json().context("知乎接口返回内容不是 JSON")?;
    if !status.is_success() || body.get("error").is_some() {
        bail!("{}", error_message(&body, status.as_u16()));
    }

    let title = pick_title(&body, &target);
    let html = body
        .get("content")
        .and_then(Value::as_str)
        .context("知乎接口没有返回正文")?;
    let text = html_to_text(html)?;
    let content = format!("{}\n\n{}", title, text);

    std::fs::create_dir_all(output_dir)?;
    let name = sanitize_filename(&title);
    let path = output_dir.join(format!("{name}.txt"));
    std::fs::write(&path, content)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_answer_and_article_links() {
        assert_eq!(
            parse_target("https://www.zhihu.com/question/123/answer/456").unwrap(),
            Target::Answer("456".into())
        );
        assert_eq!(
            parse_target("https://zhuanlan.zhihu.com/p/789").unwrap(),
            Target::Article("789".into())
        );
        assert_eq!(
            parse_target("https://www.zhihu.com/article/789").unwrap(),
            Target::Article("789".into())
        );
    }

    #[test]
    fn rejects_non_zhihu_links() {
        assert!(parse_target("https://example.com/question/1/answer/2").is_err());
    }

    #[test]
    fn extracts_cookie_values() {
        let value = extract_cookie_value("d_c0=abc; z_c0=def", "z_c0").unwrap();
        assert_eq!(value, "def");
    }
}
