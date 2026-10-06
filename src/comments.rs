use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub time: String,
    pub likes: String,
    pub like_count: Option<u64>,
    pub posted_at: Option<i64>,
    pub reply_to: String,
    pub thread: String,
}

#[derive(Clone, Debug)]
pub struct Comments {
    pub title: String,
    pub source: String,
    pub total_label: String,
    pub items: Vec<Comment>,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Sort {
    #[default]
    Original,
    OriginalReverse,
    Likes,
    LikesAscending,
    Newest,
    Oldest,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "原始顺序",
            Self::OriginalReverse => "原始倒序",
            Self::Likes => "点赞最多",
            Self::LikesAscending => "点赞最少",
            Self::Newest => "最新发布",
            Self::Oldest => "最早发布",
        }
    }
    pub fn options(self) -> (usize, bool) {
        match self {
            Self::Original => (0, false),
            Self::OriginalReverse => (0, true),
            Self::LikesAscending => (1, false),
            Self::Likes => (1, true),
            Self::Oldest => (2, false),
            Self::Newest => (2, true),
        }
    }
    pub fn from_options(basis: usize, reverse: bool) -> Self {
        match (basis, reverse) {
            (1, false) => Self::LikesAscending,
            (1, true) => Self::Likes,
            (2, false) => Self::Oldest,
            (2, true) => Self::Newest,
            (_, true) => Self::OriginalReverse,
            _ => Self::Original,
        }
    }
}

fn likes(text: &str) -> Option<u64> {
    let text = text.replace(',', "");
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let number: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let rest = text[start + number.len()..].trim().to_lowercase();
    let scale = if rest.starts_with('万') {
        10_000.0
    } else if rest.starts_with('亿') {
        100_000_000.0
    } else if rest.starts_with('k') {
        1_000.0
    } else if rest.starts_with('m') {
        1_000_000.0
    } else {
        1.0
    };
    let value = number.parse::<f64>().ok()? * scale;
    (value.is_finite() && value >= 0.0).then_some(value.round() as u64)
}

pub fn platform(input: &str) -> Option<&'static str> {
    let url = reqwest::Url::parse(input).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?;
    for (domains, name) in [
        (&["bilibili.com", "b23.tv"][..], "bili"),
        (&["douyin.com"][..], "douyin"),
        (&["xiaohongshu.com", "xhslink.com"][..], "xhs"),
        (&["zhihu.com"][..], "zhihu"),
        (&["youtube.com", "youtu.be"][..], "youtube"),
    ] {
        if domains
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        {
            return Some(name);
        }
    }
    None
}

pub fn link(input: &str) -> Option<String> {
    if let Some(url) = crate::youtube::link(input) {
        return Some(url);
    }
    if input.trim().starts_with("BV") && input.trim().chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(format!("https://www.bilibili.com/video/{}", input.trim()));
    }
    input.split_whitespace().find_map(|part| {
        let start = part.find("https://").or_else(|| part.find("http://"))?;
        let text = part[start..].trim_end_matches(['。', '，', ')', ']', ',', '.']);
        platform(text)?;
        let mut url = reqwest::Url::parse(text).ok()?;
        if platform(text) == Some("douyin") {
            if let Some(id) = url
                .query_pairs()
                .find(|(key, _)| key == "modal_id")
                .map(|(_, value)| value.into_owned())
            {
                if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                    url.set_path(&format!("/video/{id}"));
                    url.set_query(None);
                }
            }
        }
        Some(url.to_string())
    })
}

fn identity(input: &str) -> Option<String> {
    let url = reqwest::Url::parse(input).ok()?;
    let parts: Vec<_> = url.path_segments()?.collect();
    match platform(input)? {
        "youtube" => crate::youtube::link(input),
        "bili" => parts
            .windows(2)
            .find(|p| matches!(p[0], "video" | "opus" | "read" | "dynamic"))
            .map(|p| p[1].into())
            .or_else(|| {
                (url.host_str() == Some("t.bilibili.com"))
                    .then(|| parts.first().unwrap_or(&"").to_string())
                    .filter(|s| !s.is_empty())
            }),
        "douyin" => parts
            .windows(2)
            .find(|p| matches!(p[0], "video" | "note") && p[1].chars().all(|c| c.is_ascii_digit()))
            .map(|p| p[1].into()),
        "xhs" => parts
            .windows(2)
            .find(|p| matches!(p[0], "explore" | "item") && p[1].len() == 24)
            .map(|p| p[1].into()),
        "zhihu" => parts
            .windows(2)
            .rev()
            .find(|p| matches!(p[0], "answer" | "question" | "p" | "article"))
            .map(|p| format!("{}:{}", p[0], p[1])),
        _ => None,
    }
}

impl Comments {
    pub fn from_message(message: &str, expected: &str) -> Result<Self> {
        if message.len() > 4_000_000 {
            bail!("评论数据过大，请减少本次加载数量");
        }
        let data: Value = serde_json::from_str(message)?;
        let source = data["source"].as_str().context("评论来源缺失")?;
        if platform(source).is_none()
            || platform(source) != platform(expected)
            || identity(source).is_none()
        {
            bail!("请在所选内容的官方页面读取评论");
        }
        if let Some(expected_id) = identity(expected) {
            if identity(source).as_ref() != Some(&expected_id) {
                bail!("页面已切换到其他作品，请重新检测原链接的评论");
            }
        }
        let entries = data["comments"].as_array().context("没有读取到评论列表")?;
        if entries.is_empty() {
            bail!("未读取到评论，请打开评论区、展开回复后重新检测；这不代表评论数为零");
        }
        if entries.len() > 1000 {
            bail!("一次最多读取 1000 条已加载评论，请分批选择");
        }
        let mut ids = HashSet::new();
        let mut items = Vec::new();
        for entry in entries {
            let string = |key| entry[key].as_str().unwrap_or_default().trim().to_string();
            let body = string("body");
            let author = string("author");
            if body.is_empty() {
                continue;
            }
            if body.chars().count() > 20_000 {
                bail!("有评论过长，请分批获取，避免截断正文");
            }
            let reply_to = string("reply_to");
            let thread = string("thread");
            let id = string("id");
            let id = if id.is_empty() {
                format!(
                    "{:x}",
                    md5::compute(format!("{author}\n{body}\n{reply_to}\n{thread}"))
                )
            } else {
                id
            };
            if !ids.insert(id.clone()) {
                continue;
            }
            items.push(Comment {
                id,
                author,
                body,
                time: string("time"),
                likes: string("likes"),
                like_count: entry["like_count"]
                    .as_u64()
                    .or_else(|| likes(&string("likes"))),
                posted_at: entry["posted_at"].as_i64(),
                reply_to,
                thread,
            });
        }
        if items.is_empty() {
            bail!("评论正文尚未加载，请展开评论后重新检测");
        }
        Ok(Self {
            title: data["title"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("内容评论")
                .into(),
            source: source.into(),
            total_label: data["total_label"].as_str().unwrap_or_default().into(),
            items,
        })
    }

    pub fn ordered(&self, sort: Sort) -> Vec<usize> {
        let mut order: Vec<_> = (0..self.items.len()).collect();
        order.sort_by(|a, b| {
            let x = &self.items[*a];
            let y = &self.items[*b];
            let known = |x: Option<i64>, y: Option<i64>, reverse| match (x, y) {
                (Some(x), Some(y)) => {
                    if reverse {
                        y.cmp(&x)
                    } else {
                        x.cmp(&y)
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            };
            match sort {
                Sort::Original => a.cmp(b),
                Sort::OriginalReverse => b.cmp(a),
                Sort::Likes | Sort::LikesAscending => match (x.like_count, y.like_count) {
                    (Some(x), Some(y)) => {
                        if sort == Sort::Likes {
                            y.cmp(&x)
                        } else {
                            x.cmp(&y)
                        }
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                },
                Sort::Newest => known(x.posted_at, y.posted_at, true),
                Sort::Oldest => known(x.posted_at, y.posted_at, false),
            }
            .then_with(|| a.cmp(b))
        });
        order
    }

    pub fn export(
        &self,
        selected: &[bool],
        directory: &Path,
        format: crate::export::TextFormat,
        sort: Sort,
    ) -> Result<PathBuf> {
        if selected.len() != self.items.len() {
            bail!("评论选择已失效，请重新勾选");
        }
        let chosen: Vec<_> = self
            .ordered(sort)
            .into_iter()
            .filter(|i| selected[*i])
            .map(|i| (i, &self.items[i]))
            .collect();
        if chosen.is_empty() {
            bail!("请至少勾选一条评论再导出");
        }
        let mut body = format!(
            "来源：{}\n\n已检测 {} 条评论 | 本次选择 {} 条\n\n",
            self.source,
            self.items.len(),
            chosen.len()
        );
        if !self.total_label.is_empty() {
            body.push_str(&format!("页面评论数量：{}\n\n", self.total_label));
        }
        body.push_str("说明：仅包含页面已加载并由你勾选的评论，不代表全部评论。\n\n");
        if self.items.iter().any(|item| item.likes.contains('+')) {
            body.push_str(
                "点赞说明：平台显示的“10+”等为近似值，按显示数值排序，不能还原精确赞数。\n\n",
            );
        }
        body.push_str(&format!("排序：{}\n\n", sort.label()));
        for (index, item) in chosen {
            body.push_str(&format!(
                "## 评论 {} | {}\n\n",
                index + 1,
                if item.author.is_empty() {
                    "未显示作者"
                } else {
                    &item.author
                }
            ));
            if !item.thread.is_empty() {
                body.push_str(&format!("所属回答或讨论：{}\n\n", item.thread));
            }
            if !item.reply_to.is_empty() {
                body.push_str(&format!("回复：{}\n\n", item.reply_to));
            }
            if !item.time.is_empty() {
                body.push_str(&format!("时间：{}\n\n", item.time));
            }
            if !item.likes.is_empty() {
                body.push_str(&format!("赞：{}\n\n", item.likes));
            }
            body.push_str(&item.body);
            body.push_str("\n\n");
        }
        let title = format!("{} · 所选评论", self.title);
        let path = directory.join(format!(
            "{}_评论.{}",
            crate::bili::sanitize_filename(&self.title),
            format.extension()
        ));
        crate::export::save(&path, &title, &body, format)?;
        Ok(path)
    }
}

#[cfg(windows)]
pub fn open_reader(input: &str, output: &Path) -> Result<bool> {
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop},
        window::{Window, WindowId},
    };
    struct Reader {
        webview: Option<wry::WebView>,
        window: Option<Window>,
        url: String,
        proxy: winit::event_loop::EventLoopProxy<String>,
        result: Option<String>,
    }
    impl ApplicationHandler<String> for Reader {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let setup = (|| -> Result<_> {
                let profile = crate::zhihu_login::session_directory()
                    .join(format!("{}-webview", platform(&self.url).unwrap()));
                std::fs::create_dir_all(&profile)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title("拾文 · 检测评论（打开评论区，展开需要的回复后点击读取）")
                        .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 760.0)),
                )?;
                let mut context = wry::WebContext::new(Some(profile));
                let proxy = self.proxy.clone();
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_initialization_script(include_str!("comments_reader.js"))
                    .with_ipc_handler(move |request| {
                        let _ = proxy.send_event(request.body().clone());
                    })
                    .with_url(&self.url)
                    .build(&window)
                    .context("无法打开评论窗口，请检查 WebView2 Runtime")?;
                Ok((window, webview))
            })();
            match setup {
                Ok((window, webview)) => {
                    self.window = Some(window);
                    self.webview = Some(webview);
                }
                Err(error) => {
                    rfd::MessageDialog::new()
                        .set_title("检测评论")
                        .set_description(format!("{error:#}"))
                        .show();
                    event_loop.exit();
                }
            }
        }
        fn user_event(&mut self, event_loop: &ActiveEventLoop, message: String) {
            match Comments::from_message(&message, &self.url) {
                Ok(_) => {
                    self.result = Some(message);
                    event_loop.exit();
                }
                Err(error) => {
                    rfd::MessageDialog::new()
                        .set_title("检测评论")
                        .set_description(format!("{error:#}"))
                        .show();
                }
            }
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if matches!(event, WindowEvent::CloseRequested) {
                event_loop.exit();
            }
        }
    }
    let url = link(input).context("请粘贴 B站、抖音、小红书、知乎或 YouTube 的内容链接")?;
    let event_loop = EventLoop::<String>::with_user_event().build()?;
    let mut reader = Reader {
        webview: None,
        window: None,
        url,
        proxy: event_loop.create_proxy(),
        result: None,
    };
    event_loop.run_app(&mut reader)?;
    if let Some(message) = reader.result {
        std::fs::write(output, message)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(not(windows))]
pub fn open_reader(_: &str, _: &Path) -> Result<bool> {
    bail!("评论检测窗口目前支持 Windows");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_exports_only_checked_comments_and_keeps_replies() {
        let url = "https://www.douyin.com/note/123";
        let data = serde_json::json!({"source":url,"title":"测试作品","total_label":"100","comments":[
            {"id":"1","author":"甲","body":"不选的评论"},
            {"id":"2","author":"乙","body":"选中评论末尾","reply_to":"甲"},
            {"id":"2","author":"乙","body":"重复节点"}
        ]});
        let loaded = Comments::from_message(&data.to_string(), url).unwrap();
        assert_eq!(loaded.items.len(), 2);
        let dir = tempfile::tempdir().unwrap();
        assert!(loaded
            .export(
                &[false, false],
                dir.path(),
                crate::export::TextFormat::Txt,
                Sort::Original
            )
            .is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let path = loaded
            .export(
                &[false, true],
                dir.path(),
                crate::export::TextFormat::Markdown,
                Sort::Original,
            )
            .unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(!text.contains("不选的评论"));
        assert!(text.contains("选中评论末尾"));
        assert!(text.contains("回复：甲"));
        assert!(text.contains("不代表全部评论"));
    }
    #[test]
    fn validates_all_platforms_and_rejects_changed_posts() {
        for url in [
            "https://www.bilibili.com/video/BV1abc",
            "https://www.douyin.com/note/123",
            "https://www.xiaohongshu.com/explore/123456789012345678901234",
            "https://www.zhihu.com/question/123/answer/456",
            "https://www.youtube.com/watch?v=jNQXAC9IVRw",
        ] {
            let data = serde_json::json!({"source":url,"comments":[{"body":"末尾文字"}]});
            assert!(Comments::from_message(&data.to_string(), url).is_ok());
        }
        assert!(platform("https://zhihu.com.evil.test/question/123").is_none());
        let data = serde_json::json!({"source":"https://www.douyin.com/note/456","comments":[{"body":"错作品"}]});
        assert!(
            Comments::from_message(&data.to_string(), "https://www.douyin.com/note/123").is_err()
        );
        assert_eq!(
            link("https://www.douyin.com/jingxuan?modal_id=123").unwrap(),
            "https://www.douyin.com/video/123"
        );
        let empty =
            serde_json::json!({"source":"https://www.zhihu.com/question/123","comments":[]});
        assert!(
            Comments::from_message(&empty.to_string(), "https://www.zhihu.com/question/123")
                .is_err()
        );
    }
    #[test]
    fn numeric_sort_keeps_unknowns_last_and_selection_is_stable() {
        let url = "https://www.douyin.com/note/123";
        let data = serde_json::json!({"source":url,"title":"排序测试","comments":[
            {"id":"a","body":"第一条","likes":"999","posted_at":100},
            {"id":"b","body":"第二条末尾","likes":"1.2万","posted_at":300},
            {"id":"c","body":"第三条","likes":"2,000","posted_at":200},
            {"id":"d","body":"未知时间点赞"}
        ]});
        let comments = Comments::from_message(&data.to_string(), url).unwrap();
        assert_eq!(comments.ordered(Sort::Likes), vec![1, 2, 0, 3]);
        assert_eq!(comments.ordered(Sort::LikesAscending), vec![0, 2, 1, 3]);
        assert_eq!(comments.ordered(Sort::OriginalReverse), vec![3, 2, 1, 0]);
        assert_eq!(comments.ordered(Sort::Newest), vec![1, 2, 0, 3]);
        assert_eq!(comments.ordered(Sort::Oldest), vec![0, 2, 1, 3]);
        let dir = tempfile::tempdir().unwrap();
        let path = comments
            .export(
                &[true, true, false, false],
                dir.path(),
                crate::export::TextFormat::Markdown,
                Sort::Likes,
            )
            .unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.find("第二条末尾").unwrap() < text.find("第一条").unwrap());
        assert!(!text.contains("第三条"));
        let path = comments
            .export(
                &[true, true, false, false],
                dir.path(),
                crate::export::TextFormat::Markdown,
                Sort::LikesAscending,
            )
            .unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.find("第一条").unwrap() < text.find("第二条末尾").unwrap());
        for sort in [
            Sort::Original,
            Sort::OriginalReverse,
            Sort::Likes,
            Sort::LikesAscending,
            Sort::Newest,
            Sort::Oldest,
        ] {
            let (basis, reverse) = sort.options();
            assert!(Sort::from_options(basis, reverse) == sort);
        }
    }
}
