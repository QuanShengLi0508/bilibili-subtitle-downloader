use crate::bili::sanitize_filename;
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, COOKIE, REFERER, USER_AGENT};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
enum Target {
    Answer(String),
    Question(String),
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
        bail!("只支持知乎问题、回答或专栏链接");
    }

    let path = url.path();
    if let Some(id) = capture_after(path, "answer/") {
        return Ok(Target::Answer(id));
    }
    if let Some(id) = capture_after(path, "question/") {
        return Ok(Target::Question(id));
    }
    if let Some(id) = capture_after(path, "p/") {
        return Ok(Target::Article(id));
    }
    if let Some(id) = capture_after(path, "article/") {
        return Ok(Target::Article(id));
    }

    bail!("只支持知乎问题、回答或专栏链接");
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
            format!("https://api.zhihu.com/v4/answers/{id}?include=content,author,question")
        }
        Target::Question(id) => format!(
            "https://api.zhihu.com/v4/questions/{id}/answers?include=content,author,question&limit=20&offset=0"
        ),
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
            Target::Question(id) => format!("知乎问题_{id}"),
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
    if status == 404 || name == "ResourceNotFoundException" {
        return "这个知乎链接不存在或内容已删除".into();
    }
    if !message.is_empty() {
        return format!("知乎接口返回错误: {message}");
    }
    if !name.is_empty() {
        return format!("知乎接口返回错误: {name}");
    }
    format!("知乎接口请求失败（HTTP {status}）")
}

fn fetch_endpoint(endpoint: &str) -> Result<Value> {
    // The public endpoint accepts an anonymous fingerprint when the request is
    // signed consistently; this fixed value is only used to build that signature.
    let (d_c0, cookies) =
        crate::zhihu_login::request_identity(&crate::zhihu_login::saved_cookies());
    let signed = zhihu_sign::sign_zhihu_request(&endpoint, &d_c0, None);

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
    headers.insert(COOKIE, HeaderValue::from_str(&cookies)?);
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

fn answer_author(item: &Value) -> String {
    item.pointer("/author/name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("知乎用户")
        .trim()
        .to_string()
}

fn answer_content(item: &Value) -> Result<String> {
    ensure_complete(item)?;
    let html = item
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| item.get("content_html").and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty())
        .context("知乎接口没有返回回答正文")?;
    html_to_text(html)
}

fn ensure_complete(item: &Value) -> Result<()> {
    for key in [
        "content_need_truncated",
        "is_truncated",
        "is_content_truncated",
        "content_is_truncated",
    ] {
        let flag = &item[key];
        if flag == &Value::Bool(true) || flag == &serde_json::json!(1) || flag == "true" {
            bail!("知乎只返回了节选，未获取全文。请点右上角「知乎登录」，完成登录后重新获取；本次不会导出。登录后仍被截断时，请确认账号可阅读全文。");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZhihuExport {
    pub path: PathBuf,
    pub total: usize,
    pub first: usize,
    pub last: usize,
}

impl ZhihuExport {
    // Lets the GUI keep using `path.display()` while showing range info.
    pub fn display(&self) -> impl std::fmt::Display + '_ {
        struct ExportDisplay<'a>(&'a ZhihuExport);

        impl std::fmt::Display for ExportDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if self.0.total > 1 {
                    write!(
                        f,
                        "{}（共 {} 个回答，导出 {}-{}）",
                        self.0.path.display(),
                        self.0.total,
                        self.0.first,
                        self.0.last
                    )
                } else {
                    write!(f, "{}", self.0.path.display())
                }
            }
        }

        ExportDisplay(self)
    }
}

fn fetch_question_to_file(
    id: &str,
    output_dir: &Path,
    from: Option<usize>,
    to: Option<usize>,
) -> Result<ZhihuExport> {
    fetch_question_with(id, output_dir, from, to, fetch_endpoint)
}

fn fetch_question_with(
    id: &str,
    output_dir: &Path,
    from: Option<usize>,
    to: Option<usize>,
    fetch: impl FnMut(&str) -> Result<Value>,
) -> Result<ZhihuExport> {
    load_question_with(id, fetch)?.export(output_dir, from, to, crate::export::TextFormat::Txt)
}

fn load_question_with(
    id: &str,
    mut fetch: impl FnMut(&str) -> Result<Value>,
) -> Result<ZhihuContent> {
    let mut endpoint = api_url(&Target::Question(id.to_string()));
    let mut items: Vec<(usize, Value)> = Vec::new();
    let mut question_title = String::new();
    let mut total = 0usize;
    let mut ordinal = 0usize;
    let mut visited_pages = HashSet::new();
    let mut seen_answers = HashSet::new();

    loop {
        if !visited_pages.insert(endpoint.clone()) {
            bail!("知乎回答分页重复，未能获取完整列表，请稍后重试");
        }
        let body = fetch(&endpoint)?;
        if question_title.is_empty() {
            question_title = body
                .pointer("/data/0/question/title")
                .and_then(Value::as_str)
                .unwrap_or("知乎问题")
                .trim()
                .to_string();
        }
        total = total.max(
            body.pointer("/paging/totals")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        );

        let page = body
            .get("data")
            .and_then(Value::as_array)
            .context("知乎接口没有返回回答列表")?;
        let previous_ordinal = ordinal;
        for item in page {
            let answer_id = value_id(item.get("id").unwrap_or(&Value::Null))
                .context("知乎回答缺少编号，无法确认完整列表")?;
            if !seen_answers.insert(answer_id) {
                continue;
            }
            ordinal += 1;
            items.push((ordinal, item.clone()));
        }

        if body
            .pointer("/paging/is_end")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            break;
        }
        if ordinal == previous_ordinal {
            bail!("知乎未返回新的回答，未能获取完整列表，请稍后重试");
        }

        let next = body
            .pointer("/paging/next")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .context("知乎回答分页链接缺失")?
            .to_string();
        let next_url = reqwest::Url::parse(&endpoint)?.join(&next)?;
        let host = next_url.host_str().unwrap_or_default();
        if !matches!(next_url.scheme(), "http" | "https")
            || !(host == "zhihu.com" || host.ends_with(".zhihu.com"))
        {
            bail!("知乎回答分页链接不正确");
        }
        endpoint = next_url.to_string();
    }

    if items.is_empty() {
        bail!("这个问题没有可获取的回答");
    }
    total = total.max(ordinal);
    let sections = items
        .iter()
        .map(|(ordinal, item)| {
            let answer_id = value_id(item.get("id").unwrap_or(&Value::Null)).unwrap_or_default();
            let link = format!("https://www.zhihu.com/question/{id}/answer/{answer_id}");
            Ok(format!(
                "回答 {ordinal} | {}\n链接：{link}\n\n{}",
                answer_author(item),
                answer_content(item)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ZhihuContent {
        title: question_title,
        sections,
        total,
        is_question: true,
    })
}

#[derive(Debug, Clone)]
pub struct ZhihuContent {
    pub title: String,
    pub sections: Vec<String>,
    pub total: usize,
    pub is_question: bool,
}

impl ZhihuContent {
    pub fn count(&self) -> usize {
        self.sections.len()
    }

    pub fn export(
        &self,
        output_dir: &Path,
        from: Option<usize>,
        to: Option<usize>,
        format: crate::export::TextFormat,
    ) -> Result<ZhihuExport> {
        let first = from.unwrap_or(1);
        let last = to.unwrap_or(self.count());
        if first == 0 || last == 0 || first > last || last > self.count() {
            bail!(
                "导出范围必须在 1-{} 之间，且起始序号不能大于结束序号",
                self.count()
            );
        }
        let selected = self.sections[first - 1..last].join("\n\n---\n\n");
        let body = if self.is_question {
            format!(
                "已获取 {} 个回答 | 导出 {first}-{last}\n\n{selected}",
                self.count()
            )
        } else {
            selected
        };
        let filename = if self.is_question {
            format!("{}_回答{first}-{last}", sanitize_filename(&self.title))
        } else {
            sanitize_filename(&self.title)
        };
        let path = output_dir.join(format!("{filename}.{}", format.extension()));
        crate::export::save(&path, &self.title, &body, format)?;
        Ok(ZhihuExport {
            path,
            total: self.count(),
            first,
            last,
        })
    }
}

pub fn fetch_content(input: &str, all_answers: bool) -> Result<ZhihuContent> {
    let target = parse_target(input)?;
    let question = match &target {
        Target::Question(id) => Some(id.clone()),
        Target::Answer(_) if all_answers => Some(match question_id_from_link(input) {
            Some(id) => id,
            None => {
                let body = fetch_endpoint(&api_url(&target))?;
                value_id(body.pointer("/question/id").unwrap_or(&Value::Null))
                    .context("无法识别这个回答所属的问题")?
            }
        }),
        _ => None,
    };
    if let Some(id) = question {
        return load_question_with(&id, fetch_endpoint);
    }
    let body = fetch_endpoint(&api_url(&target))?;
    Ok(ZhihuContent {
        title: pick_title(&body, &target),
        sections: vec![answer_content(&body)?],
        total: 1,
        is_question: false,
    })
}

fn value_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) => {
            Some(id.clone())
        }
        Value::Number(id) if id.is_u64() => Some(id.to_string()),
        _ => None,
    }
}

fn question_id_from_link(input: &str) -> Option<String> {
    let normalized = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    reqwest::Url::parse(&normalized)
        .ok()
        .and_then(|url| capture_after(url.path(), "question/"))
}

pub fn fetch_to_file_scope(
    input: &str,
    output_dir: &Path,
    from: Option<usize>,
    to: Option<usize>,
    all_answers: bool,
) -> Result<ZhihuExport> {
    let target = parse_target(input)?;
    if all_answers {
        if let Target::Answer(_) = &target {
            let id = match question_id_from_link(input) {
                Some(id) => id,
                None => {
                    let body = fetch_endpoint(&api_url(&target))?;
                    value_id(body.pointer("/question/id").unwrap_or(&Value::Null))
                        .context("无法识别这个回答所属的问题")?
                }
            };
            return fetch_question_to_file(&id, output_dir, from, to);
        }
    }
    fetch_to_file_range(input, output_dir, from, to)
}

pub fn fetch_to_file_range(
    input: &str,
    output_dir: &Path,
    from: Option<usize>,
    to: Option<usize>,
) -> Result<ZhihuExport> {
    let target = parse_target(input)?;
    if let Target::Question(id) = &target {
        return fetch_question_to_file(id, output_dir, from, to);
    }

    let body = fetch_endpoint(&api_url(&target))?;
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
    Ok(ZhihuExport {
        path,
        total: 1,
        first: 1,
        last: 1,
    })
}

pub fn fetch_to_file(input: &str, output_dir: &Path) -> Result<PathBuf> {
    Ok(fetch_to_file_range(input, output_dir, None, None)?.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_article_and_answers_cannot_be_exported() {
        let excerpt = serde_json::json!({"content": "<p>被截断的正文</p>", "content_need_truncated": true, "force_login_when_click_read_more": true});
        assert!(answer_content(&excerpt)
            .unwrap_err()
            .to_string()
            .contains("知乎登录"));
        let question = load_question_with("123", |_| {
            Ok(serde_json::json!({
                "data": [{"id": "456", "content": "<p>回答节选</p>", "content_need_truncated": true}],
                "paging": {"is_end": true}
            }))
        });
        assert!(question.is_err());
        let complete = serde_json::json!({"content": format!("<p>{}</p><p>最后一段完整保留。</p>", "长段落内容。".repeat(300)), "content_need_truncated": false});
        assert!(answer_content(&complete)
            .unwrap()
            .ends_with("最后一段完整保留。"));
    }

    #[test]
    fn preview_is_kept_in_memory_and_exports_only_confirmed_range() {
        let dir = tempfile::tempdir().unwrap();
        let preview = load_question_with("123", |_| Ok(serde_json::json!({
            "data": (1..=4).map(|id| serde_json::json!({"id":id,"question":{"title":"预览测试"},"content":format!("<p>正文{id}</p>")})).collect::<Vec<_>>(),
            "paging":{"is_end":true,"totals":4}
        }))).unwrap();
        assert_eq!(preview.count(), 4);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(preview
            .export(dir.path(), Some(0), Some(2), crate::export::TextFormat::Txt)
            .is_err());
        assert!(preview
            .export(dir.path(), Some(2), Some(5), crate::export::TextFormat::Txt)
            .is_err());
        assert!(preview
            .export(dir.path(), Some(3), Some(2), crate::export::TextFormat::Txt)
            .is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let saved = preview
            .export(
                dir.path(),
                Some(2),
                Some(3),
                crate::export::TextFormat::Markdown,
            )
            .unwrap();
        let text = std::fs::read_to_string(saved.path).unwrap();
        assert!(text.contains("正文2") && text.contains("正文3"));
        assert!(!text.contains("正文1") && !text.contains("正文4"));
        assert_eq!((saved.first, saved.last, saved.total), (2, 3, 4));
    }

    #[test]
    fn exports_more_answers_across_pages_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let answer = |id: u64| serde_json::json!({"id": id, "question": {"title":"分页问题"}, "author":{"name":format!("作者{id}")}, "content": format!("<p>回答正文{id}</p>")});
        let mut calls = 0;
        let export = fetch_question_with("123", dir.path(), None, None, |url| {
            calls += 1;
            Ok(if calls == 1 {
                serde_json::json!({"data":[answer(900000000000000001),answer(2)],"paging":{"totals":3,"is_end":false,"next":"/v4/questions/123/answers?offset=2"}})
            } else {
                assert!(url.ends_with("offset=2"));
                serde_json::json!({"data":[answer(2),answer(3)],"paging":{"totals":3,"is_end":true}})
            })
        }).unwrap();
        assert_eq!(calls, 2);
        assert_eq!((export.total, export.first, export.last), (3, 1, 3));
        let text = std::fs::read_to_string(export.path).unwrap();
        assert!(text.contains("回答 3 | 作者3"));
        assert_eq!(text.matches("回答正文2").count(), 1);
        assert!(text.contains("/answer/900000000000000001"));
        assert_eq!(
            question_id_from_link("https://www.zhihu.com/question/123/answer/456"),
            Some("123".into())
        );
    }

    #[test]
    fn refuses_incomplete_repeated_pagination() {
        let dir = tempfile::tempdir().unwrap();
        let error = fetch_question_with("123", dir.path(), None, None, |url| {
            Ok(serde_json::json!({
                "data":[{"id":"1","content":"<p>正文</p>"}],
                "paging":{"is_end":false,"next":url}
            }))
        })
        .unwrap_err();
        assert!(error.to_string().contains("分页重复"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    #[ignore = "需要网络，验证用户反馈的更多回答"]
    fn fetches_reported_answer_with_all_answers() {
        let dir = tempfile::tempdir().unwrap();
        let preview = fetch_content(
            "https://www.zhihu.com/question/2059626138549350826/answer/2072289562546774195",
            true,
        )
        .unwrap();
        println!(
            "预览获取 {} 条回答，接口总数 {}",
            preview.count(),
            preview.total
        );
        assert!(preview.count() > 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let export = preview
            .export(dir.path(), Some(2), Some(3), crate::export::TextFormat::Pdf)
            .unwrap();
        assert_eq!((export.first, export.last), (2, 3));
        assert!(std::fs::metadata(export.path).unwrap().len() > 100);
    }

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
    fn parses_question_links() {
        assert_eq!(
            parse_target("https://www.zhihu.com/question/123").unwrap(),
            Target::Question("123".into())
        );
    }

    #[test]
    fn rejects_non_zhihu_links() {
        assert!(parse_target("https://example.com/question/1/answer/2").is_err());
    }

    #[test]
    #[ignore = "需要网络，验证问题回答范围导出"]
    fn fetches_question_range() {
        let dir = std::path::Path::new("target/tmp/zhihu-test");
        let export = fetch_to_file_range(
            "https://www.zhihu.com/question/67287444",
            dir,
            Some(1),
            Some(5),
        )
        .unwrap();
        assert_eq!(export.first, 1);
        assert_eq!(export.last, 5);
        let text = std::fs::read_to_string(&export.path).unwrap();
        assert!(text.contains("导出 1-5"));
        assert!(text.contains("回答 5 |"));
        assert!(!text.contains("回答 6 |"));
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
