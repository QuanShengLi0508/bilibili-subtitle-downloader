use crate::bili::sanitize_filename;
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, COOKIE, REFERER, USER_AGENT};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
enum Target {
    Answer(String),
    Article(String),
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

    let path = url.path();
    if let Some(id) = capture_after(path, "answer/") {
        return Ok(Target::Answer(id));
    }
    if let Some(id) = capture_after(path, "p/") {
        return Ok(Target::Article(id));
    }
    if let Some(id) = capture_after(path, "article/") {
        return Ok(Target::Article(id));
    }

    bail!("只支持知乎回答链接或专栏链接");
}

fn capture_after(path: &str, marker: &str) -> Option<String> {
    let start = path.find(marker)? + marker.len();
    let rest = &path[start..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn api_url(target: &Target) -> String {
    match target {
        Target::Answer(id) => {
            format!("https://www.zhihu.com/api/v4/answers/{id}?include=data%5B*%5D.content")
        }
        Target::Article(id) => format!("https://zhuanlan.zhihu.com/api/articles/{id}"),
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
        bail!("知乎接口没有返回正文");
    }
    Ok(text)
}

fn pick_title(value: &Value, target: &Target) -> String {
    let title = value
        .pointer("/question/title")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            value
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            value
                .get("excerpt_title")
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
        .pointer("/error/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if name == "1001" || name.eq_ignore_ascii_case("need_login") {
        return "知乎暂时限制了这个链接，请稍后再试".into();
    }
    if !message.is_empty() {
        return format!("知乎接口返回错误: {message}");
    }
    if !name.is_empty() {
        return format!("知乎接口返回错误: {name}");
    }
    format!("知乎接口请求失败（HTTP {status}）")
}

fn fetch_json(target: &Target) -> Result<Value> {
    let endpoint = api_url(target);
    // The public endpoint accepts an anonymous fingerprint when the request is
    // signed consistently; this fixed value is only used to build that signature.
    let d_c0 = "ZhihuAnonymousFingerprint00000000000000";
    let signed = zhihu_sign::sign_zhihu_request(&endpoint, d_c0, None);

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36 Edg/123.0.0.0"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(
        HeaderName::from_static("accept-language"),
        HeaderValue::from_static("zh-CN,zh;q=0.9"),
    );
    headers.insert(
        HeaderName::from_static("origin"),
        HeaderValue::from_static("https://zhuanlan.zhihu.com"),
    );
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://zhuanlan.zhihu.com/"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::from_static(
            "\"Microsoft Edge\";v=\"123\", \"Not:A-Brand\";v=\"8\", \"Chromium\";v=\"123\"",
        ),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-mobile"),
        HeaderValue::from_static("?0"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-platform"),
        HeaderValue::from_static("\"Windows\""),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("empty"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("cors"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-site"),
    );
    headers.insert(COOKIE, HeaderValue::from_str(&format!("d_c0={d_c0}"))?);
    for (name, value) in signed {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes())?,
            HeaderValue::from_str(&value)?,
        );
    }

    let response = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(endpoint)
        .headers(headers)
        .send()?;
    let status = response.status();
    let text = response.text().context("读取知乎接口响应失败")?;
    let body: Value = serde_json::from_str(&text)
        .with_context(|| format!("知乎接口返回内容不是 JSON（HTTP {}）", status.as_u16()))?;

    if !status.is_success() || body.get("error").is_some() {
        bail!("{}", error_message(&body, status.as_u16()));
    }
    Ok(body)
}

pub fn fetch_to_file(input: &str, output_dir: &Path) -> Result<PathBuf> {
    let target = parse_target(input)?;
    let body = fetch_json(&target)?;
    let title = pick_title(&body, &target);
    let html = body
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| body.get("content_html").and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty())
        .context("知乎接口没有返回正文")?;
    let text = html_to_text(html)?;
    let content = format!("{title}\n\n{text}");

    std::fs::create_dir_all(output_dir)?;
    let path = output_dir.join(format!("{}.txt", sanitize_filename(&title)));
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
}
#[test]
#[ignore = "需要网络，验证真实知乎链接"]
fn fetches_real_answer_and_article() {
    let dir = std::path::Path::new("target/tmp/zhihu-test");
    let answer = fetch_to_file(
        "https://www.zhihu.com/question/67287444/answer/251460831",
        dir,
    )
    .unwrap();
    assert!(std::fs::read_to_string(answer).unwrap().len() > 100);

    let article = fetch_to_file("https://zhuanlan.zhihu.com/p/2048549405612041923", dir).unwrap();
    assert!(std::fs::read_to_string(article).unwrap().len() > 100);
}
