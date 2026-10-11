use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn official_host(host: &str) -> bool {
    matches!(
        host,
        "x.com"
            | "www.x.com"
            | "mobile.x.com"
            | "m.x.com"
            | "twitter.com"
            | "www.twitter.com"
            | "mobile.twitter.com"
            | "m.twitter.com"
    )
}

fn parse_link(input: &str) -> Option<(String, String)> {
    let url = reqwest::Url::parse(input).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !official_host(url.host_str()?)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let parts: Vec<_> = url
        .path()
        .trim_end_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    let (id, suffix) = match parts.as_slice() {
        ["i", "web", "status", id, rest @ ..] => (*id, rest),
        ["statuses", id, rest @ ..] => (*id, rest),
        [user, "status", id, rest @ ..]
            if !user.is_empty()
                && user.len() <= 15
                && user.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') =>
        {
            (*id, rest)
        }
        _ => return None,
    };
    if id.is_empty()
        || id.len() > 20
        || id.starts_with('0')
        || !id.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    if !suffix.is_empty()
        && !matches!(suffix, ["photo" | "video", n] if n.parse::<u8>().is_ok_and(|v| v > 0 && v <= 16))
    {
        return None;
    }
    Some((format!("https://x.com/i/web/status/{id}"), id.to_owned()))
}

pub fn link(input: &str) -> Option<String> {
    for (index, _) in input.match_indices("http") {
        let candidate = input[index..]
            .split(|c: char| c.is_whitespace() || "<>\"'，。；！【】（）《》“”".contains(c))
            .next()?
            .trim_end_matches([')', ']', '}', ',', '.', ';', '!']);
        if let Some((url, _)) = parse_link(candidate) {
            return Some(url);
        }
    }
    None
}

fn id(input: &str) -> Option<String> {
    parse_link(input).map(|(_, id)| id)
}

fn photo_url(input: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(input).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("pbs.twimg.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !url.path().starts_with("/media/")
    {
        return None;
    }
    let format = url
        .query_pairs()
        .find(|(key, _)| key == "format")
        .map(|(_, v)| v.into_owned());
    if let Some(format) = format {
        if !matches!(format.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif") {
            return None;
        }
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("format", &format)
            .append_pair("name", "orig");
    } else {
        url.set_query(Some("name=orig"));
    }
    url.set_fragment(None);
    Some(url.to_string())
}

#[derive(Clone, Debug)]
pub struct Post {
    pub title: String,
    pub body: String,
    pub author: String,
    pub source: String,
    pub published_at: String,
    pub images: Vec<String>,
    pub video_count: usize,
    pub warning: Option<String>,
}

fn post_title(body: &str, author: &str, post_id: &str) -> String {
    let summary: String = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(55)
        .collect();
    if summary.is_empty() {
        format!("X 帖子 {post_id}")
    } else if author.is_empty() {
        summary
    } else {
        format!("{author} - {summary}")
    }
}

fn syndication_post(value: &Value, source: &str) -> Result<Post> {
    let post_id = id(source).context("X 帖子链接无效")?;
    if value["id_str"].as_str() != Some(&post_id) {
        bail!("X 未返回所选帖子，可能已删除、受限或需要登录；可尝试官方网页读取");
    }
    if value["truncated"].as_bool() == Some(true) || value["is_note_tweet"].as_bool() == Some(true)
    {
        bail!("X 公开预览只返回了长文摘要，请使用「官方网页读取」展开原帖再获取");
    }
    let body = value["full_text"]
        .as_str()
        .or_else(|| value["text"].as_str())
        .unwrap_or_default()
        .to_owned();
    let author = value["user"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let mut images = Vec::new();
    let mut video_count = 0;
    if let Some(media) = value["mediaDetails"].as_array() {
        for item in media {
            match item["type"].as_str() {
                Some("photo") => {
                    let photo = photo_url(
                        item["media_url_https"]
                            .as_str()
                            .or_else(|| item["media_url"].as_str())
                            .unwrap_or_default(),
                    )
                    .context("X 返回的配图来源无效，未忽略这张配图")?;
                    if !images.contains(&photo) {
                        images.push(photo);
                    }
                }
                Some("video" | "animated_gif") => video_count += 1,
                _ => {}
            }
        }
    } else if let Some(photos) = value["photos"].as_array() {
        for item in photos {
            let photo =
                photo_url(item["url"].as_str().unwrap_or_default()).context("X 配图来源无效")?;
            if !images.contains(&photo) {
                images.push(photo);
            }
        }
    }
    if video_count == 0 && value["video"].is_object() {
        video_count = 1;
    }
    if body.trim().is_empty() && images.is_empty() && video_count == 0 {
        bail!("X 返回了空帖子，未导出");
    }
    Ok(Post {
        title: post_title(&body, &author, &post_id), body, author,
        source: link(source).unwrap(),
        published_at: value["created_at"].as_str().unwrap_or_default().into(), images, video_count,
        warning: Some("当前为 X 官方公开预览；长文和部分媒体可能省略。可用「官方网页读取」核对原帖，引用帖和回复不合并。".into()),
    })
}

impl Post {
    pub fn from_message(message: &str, expected: &str) -> Result<Self> {
        if message.len() > 4_000_000 {
            bail!("X 帖子数据过大");
        }
        let value: Value = serde_json::from_str(message)?;
        let source = value["source"].as_str().unwrap_or_default();
        if value["kind"] != "twitter-post" || id(expected).is_none() || id(expected) != id(source) {
            bail!("当前页面不是所选的 X 帖子");
        }
        if value["complete"].as_bool() != Some(true) {
            bail!("原帖仍有展开、登录或访问限制，请先显示完整正文");
        }
        let body = value["body"].as_str().unwrap_or_default().trim().to_owned();
        let author = value["author"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();
        let image_values = value["images"].as_array().context("帖子配图数据缺失")?;
        if image_values.len() > 50 {
            bail!("帖子配图数量异常，未保存");
        }
        let mut images = Vec::new();
        for image in image_values {
            let photo =
                photo_url(image.as_str().unwrap_or_default()).context("帖子配图不是 X 官方图片");
            let photo = photo?;
            if !images.contains(&photo) {
                images.push(photo);
            }
        }
        let video_count = value["video_count"].as_u64().unwrap_or(0).min(16) as usize;
        if body.is_empty() && images.is_empty() && video_count == 0 {
            bail!("没有读取到原帖文字或媒体");
        }
        Ok(Self {
            title: post_title(&body, &author, &id(source).unwrap()),
            body,
            author,
            source: link(source).unwrap(),
            images,
            video_count,
            published_at: value["published_at"].as_str().unwrap_or_default().into(),
            warning: Some("仅导出当前原帖；引用帖、回复和图片内文字不合并。".into()),
        })
    }

    pub fn export(&self, directory: &Path, format: crate::export::TextFormat) -> Result<PathBuf> {
        std::fs::create_dir_all(directory)?;
        let name = crate::bili::sanitize_filename(&self.title);
        let post_id = id(&self.source).context("帖子来源无效")?;
        let folder = tempfile::Builder::new()
            .prefix(&format!("{name}_X_{post_id}_"))
            .tempdir_in(directory)?;
        let mut text = format!(
            "作者：{}\n发布时间：{}\n来源：{}\n\n{}",
            self.author, self.published_at, self.source, self.body
        );
        if let Some(warning) = &self.warning {
            text.push_str(&format!("\n\n导出范围：{warning}"));
        }
        if self.video_count > 0 {
            text.push_str(&format!(
                "\n\n原帖包含 {} 个视频；视频可另行下载。",
                self.video_count
            ));
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(45))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 4 && photo_url(attempt.url().as_str()).is_some() {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()?;
        for (index, url) in self.images.iter().enumerate() {
            let url = photo_url(url).context("帖子图片链接无效")?;
            let response = client
                .get(&url)
                .header("Referer", "https://x.com/")
                .send()?
                .error_for_status()?;
            let mut bytes = Vec::new();
            response.take(50_000_001).read_to_end(&mut bytes)?;
            if bytes.len() > 50_000_000 {
                bail!("第 {} 张配图过大，未保存", index + 1);
            }
            let ext = image_extension(&bytes)
                .with_context(|| format!("第 {} 张配图下载失败，未返回图片", index + 1))?;
            let filename = format!("配图{:02}.{ext}", index + 1);
            std::fs::write(folder.path().join(&filename), bytes)?;
            text.push_str(&format!("\n\n![配图{}]({filename})", index + 1));
        }
        crate::export::save(
            &folder
                .path()
                .join(format!("帖子正文.{}", format.extension())),
            &self.title,
            &text,
            format,
        )?;
        Ok(folder.keep())
    }
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else {
        None
    }
}

pub fn cookie_path() -> PathBuf {
    crate::zhihu_login::session_directory().join("twitter-cookies.txt")
}

fn cookie_contents(text: &str) -> Result<String> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let row = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if row.starts_with('#') || row.trim().is_empty() {
            continue;
        }
        let fields: Vec<_> = row.split('\t').collect();
        if fields.len() != 7 {
            continue;
        }
        let domain = fields[0].trim_start_matches('.').to_ascii_lowercase();
        if !["x.com", "twitter.com"]
            .iter()
            .any(|d| domain == *d || domain.ends_with(&format!(".{d}")))
        {
            continue;
        }
        if !matches!(fields[1], "TRUE" | "FALSE")
            || !matches!(fields[3], "TRUE" | "FALSE")
            || !fields[2].starts_with('/')
            || fields[4].parse::<u64>().is_err()
            || fields[5].is_empty()
            || fields[5..].iter().any(|s| s.contains(['\r', '\n', '\0']))
        {
            continue;
        }
        rows.push(line.to_owned());
    }
    if rows.is_empty() {
        bail!("没有找到 X / Twitter 登录状态，请选择 Netscape 格式 cookies.txt 文件");
    }
    Ok(format!(
        "# Netscape HTTP Cookie File\n{}\n",
        rows.join("\n")
    ))
}

pub fn import_cookies(path: &Path) -> Result<()> {
    if std::fs::metadata(path)?.len() > 2_000_000 {
        bail!("登录状态文件过大");
    }
    let content = cookie_contents(&std::fs::read_to_string(path)?)?;
    let output = cookie_path();
    std::fs::create_dir_all(output.parent().context("登录状态目录不存在")?)?;
    let mut file = tempfile::NamedTempFile::new_in(output.parent().unwrap())?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(output).context("保存 X 登录状态失败")?;
    Ok(())
}

fn hide_console(command: &mut Command) {
    command.env("PYTHONIOENCODING", "utf-8");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}

fn run_bounded(command: &mut Command, timeout: Duration, progress: &dyn Fn(f64)) -> Result<Output> {
    let stdout = tempfile::NamedTempFile::new()?;
    let stderr = tempfile::NamedTempFile::new()?;
    hide_console(command);
    let mut child = command
        .stdout(Stdio::from(stdout.reopen()?))
        .stderr(Stdio::from(stderr.reopen()?))
        .spawn()
        .context("无法启动 X 读取工具")?;
    let start = Instant::now();
    let mut last_poll = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            #[cfg(windows)]
            {
                let mut stop = Command::new("taskkill.exe");
                hide_console(&mut stop);
                let _ = stop
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
            bail!("X 操作超时，请检查网络或导入登录状态后重试");
        }
        if last_poll.elapsed() >= Duration::from_millis(700) {
            let mut text = String::new();
            stdout
                .reopen()?
                .take(4_000_000)
                .read_to_string(&mut text)
                .ok();
            if let Some(value) = text.lines().rev().find_map(|line| {
                line.strip_prefix("shiwen_progress:")
                    .and_then(|v| v.trim().trim_end_matches('%').parse::<f64>().ok())
            }) {
                progress((value / 100.0).clamp(0.0, 1.0));
            }
            last_poll = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut out = Vec::new();
    stdout.reopen()?.take(12_000_001).read_to_end(&mut out)?;
    let mut err = Vec::new();
    stderr.reopen()?.take(1_000_001).read_to_end(&mut err)?;
    if out.len() > 12_000_000 || err.len() > 1_000_000 {
        bail!("X 工具返回的数据过大");
    }
    Ok(Output {
        status,
        stdout: out,
        stderr: err,
    })
}

fn configure_tool(command: &mut Command) {
    command.args([
        "--ignore-config",
        "--no-warnings",
        "--socket-timeout",
        "20",
        "--retries",
        "1",
        "--extractor-retries",
        "1",
        "--fragment-retries",
        "1",
    ]);
    if cookie_path().is_file() {
        command.arg("--cookies").arg(cookie_path());
    }
}

fn tool_error(stderr: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(stderr).to_lowercase();
    if text.contains("login")
        || text.contains("auth")
        || text.contains("403")
        || text.contains("protected")
    {
        "X 要求登录或该帖子受限，请导入自己的登录状态后重试；也可在官方网页读取可见内容"
    } else if text.contains("429") || text.contains("rate-limit") {
        "X 暂时限制访问频率，请稍后再试"
    } else if text.contains("no video") || text.contains("not a video") {
        "这条 X 帖子没有可下载的视频，可切换到图文 / 文字导出"
    } else if text.contains("unavailable") || text.contains("deleted") || text.contains("404") {
        "这条 X 帖子暂不可访问，可能已删除或需要登录"
    } else {
        "X 获取失败，请检查网络、导入登录状态，或尝试官方网页读取"
    }
}

fn tool_metadata(source: &str, tool: &Path) -> Result<Post> {
    let mut command = Command::new(tool);
    configure_tool(&mut command);
    command
        .args([
            "--dump-single-json",
            "--yes-playlist",
            "--use-extractors",
            "Twitter",
        ])
        .arg(source);
    let output = run_bounded(&mut command, Duration::from_secs(90), &|_| {})?;
    if !output.status.success() {
        bail!("{}", tool_error(&output.stderr));
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let entries: Vec<&Value> = value["entries"]
        .as_array()
        .map(|v| v.iter().collect())
        .unwrap_or_else(|| vec![&value]);
    if entries.is_empty() {
        bail!("X 没有返回视频内容");
    }
    let post_id = id(source).context("X 帖子链接无效")?;
    if entries
        .iter()
        .any(|entry| entry["display_id"].as_str() != Some(&post_id))
    {
        bail!("视频信息与所选 X 帖子不匹配");
    }
    let first = entries[0];
    let body = first["description"].as_str().unwrap_or_default().to_owned();
    let author = first["uploader"].as_str().unwrap_or_default().to_owned();
    Ok(Post {
        title: post_title(&body, &author, &post_id),
        body,
        author,
        source: link(source).unwrap(),
        published_at: first["upload_date"].as_str().unwrap_or_default().into(),
        images: vec![],
        video_count: entries.len(),
        warning: Some(
            "当前文字来自视频说明，可能省略长文或配图；图文导出前请使用「官方网页读取」核对。"
                .into(),
        ),
    })
}

pub fn probe(input: &str, tool: &Path) -> Result<Post> {
    let source = link(input).context("请粘贴 X / Twitter 帖子链接或包含链接的分享文字")?;
    let public = (|| -> Result<Post> {
        let post_id = id(&source).unwrap();
        let node = tool.parent().unwrap_or(Path::new(".")).join("node.exe");
        let mut command = Command::new(node);
        command.args(["-e", "process.stdout.write(((Number(process.argv[1])/1e15)*Math.PI).toString(36).replace(/(0+|\\.)/g,''))", &post_id]);
        let result = run_bounded(&mut command, Duration::from_secs(8), &|_| {})?;
        if !result.status.success() {
            bail!("公开预览服务暂不可用");
        }
        let token = String::from_utf8(result.stdout)?;
        if token.is_empty() || !token.bytes().all(|c| c.is_ascii_alphanumeric()) {
            bail!("公开预览参数无效");
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("Shiwen/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(25))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let response = client
            .get("https://cdn.syndication.twimg.com/tweet-result")
            .query(&[("id", post_id.as_str()), ("token", token.as_str())])
            .send()?
            .error_for_status()?;
        let mut bytes = Vec::new();
        response.take(4_000_001).read_to_end(&mut bytes)?;
        if bytes.len() > 4_000_000 {
            bail!("X 公开预览数据过大");
        }
        syndication_post(&serde_json::from_slice(&bytes)?, &source)
    })();
    match public {
        Ok(post) => Ok(post),
        Err(error) if error.to_string().contains("长文摘要") => Err(error),
        Err(error) => {
            #[cfg(test)]
            eprintln!("X public-preview failure: {error:#}");
            #[cfg(not(test))]
            let _ = error;
            tool_metadata(&source, tool)
        }
    }
}

pub fn download(
    post: &Post,
    tool: &Path,
    directory: &Path,
    progress: &dyn Fn(f64),
) -> Result<Vec<PathBuf>> {
    if post.video_count == 0 {
        bail!("该帖子未检测到视频，请先获取视频信息");
    }
    let source = link(&post.source).context("X 帖子链接无效")?;
    std::fs::create_dir_all(directory)?;
    let folder = tempfile::Builder::new()
        .prefix(&format!(
            "{}_X视频_",
            crate::bili::sanitize_filename(&post.title)
        ))
        .tempdir_in(directory)?;
    let mut command = Command::new(tool);
    configure_tool(&mut command);
    command
        .args([
            "--yes-playlist",
            "--use-extractors",
            "Twitter",
            "--playlist-items",
        ])
        .arg(format!("1:{}", post.video_count.min(16)))
        .args([
            "--newline",
            "--no-simulate",
            "--progress",
            "--progress-template",
            "download:shiwen_progress:%(progress._percent_str)s",
            "-f",
            "bv*+ba/b",
            "--merge-output-format",
            "mp4",
            "--print",
            "after_move:filepath",
            "--paths",
        ])
        .arg(folder.path())
        .args(["-o", "%(playlist_index|1)02d_%(id)s.%(ext)s"]);
    if let Some(parent) = tool.parent() {
        command.arg("--ffmpeg-location").arg(parent);
    }
    command.arg(&source);
    let output = run_bounded(&mut command, Duration::from_secs(3600), progress)?;
    if !output.status.success() {
        bail!("{}", tool_error(&output.stderr));
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(folder.path())? {
        let path = entry?.path();
        if path.is_file()
            && path.metadata()?.len() > 0
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| matches!(ext, "mp4" | "mkv" | "webm" | "mov"))
        {
            files.push(path);
        }
    }
    if files.is_empty() {
        bail!("X 未返回有效视频文件，未保存");
    }
    files.sort();
    let _ = folder.keep();
    progress(1.0);
    Ok(files)
}

pub fn read(input: &str) -> Result<Post> {
    let url = link(input).context("请粘贴 X / Twitter 帖子链接")?;
    let preview = tempfile::NamedTempFile::new()?;
    let mut command = Command::new(std::env::current_exe()?);
    hide_console(&mut command);
    let status = command
        .arg("--twitter-reader")
        .arg(&url)
        .arg(preview.path())
        .status()?;
    if status.code() == Some(2) {
        bail!("已取消 X 帖子读取");
    }
    if !status.success() {
        bail!("X 帖子窗口读取失败");
    }
    Post::from_message(&std::fs::read_to_string(preview.path())?, &url)
}

fn valid_ipc_source(message: &str, actual: &str, expected: &str) -> bool {
    actual.starts_with("https://")
        && id(actual).is_some()
        && id(actual) == id(expected)
        && Post::from_message(message, expected).is_ok()
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
        result: Option<Result<Option<String>>>,
    }
    impl ApplicationHandler<String> for Reader {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let setup = (|| -> Result<_> {
                let profile = crate::zhihu_login::session_directory().join("twitter-webview");
                std::fs::create_dir_all(&profile)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title("拾文 · X / Twitter（原帖显示完整后获取）")
                        .with_inner_size(winit::dpi::LogicalSize::new(1080.0, 760.0)),
                )?;
                let mut context = wry::WebContext::new(Some(profile));
                let proxy = self.proxy.clone();
                let expected = self.url.clone();
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_initialization_script(include_str!("twitter_reader.js"))
                    .with_navigation_handler(|input| {
                        reqwest::Url::parse(&input).is_ok_and(|u| {
                            u.scheme() == "https"
                                && u.host_str().is_some_and(official_host)
                                && u.username().is_empty()
                                && u.password().is_none()
                                && u.port().is_none()
                        })
                    })
                    .with_ipc_handler(move |request| {
                        if valid_ipc_source(request.body(), &request.uri().to_string(), &expected) {
                            let _ = proxy.send_event(request.body().clone());
                        }
                    })
                    .with_url(&self.url)
                    .build(&window)
                    .context("无法打开 X 窗口，请检查 WebView2 Runtime")?;
                Ok((window, webview))
            })();
            match setup {
                Ok((window, webview)) => {
                    self.window = Some(window);
                    self.webview = Some(webview);
                }
                Err(e) => {
                    self.result = Some(Err(e));
                    event_loop.exit();
                }
            }
        }
        fn user_event(&mut self, event_loop: &ActiveEventLoop, message: String) {
            match Post::from_message(&message, &self.url) {
                Ok(_) => {
                    self.result = Some(Ok(Some(message)));
                    event_loop.exit();
                }
                Err(e) => {
                    rfd::MessageDialog::new()
                        .set_title("X / Twitter")
                        .set_description(e.to_string())
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
    let url = link(input).context("请粘贴 X / Twitter 帖子链接")?;
    let event_loop = EventLoop::<String>::with_user_event().build()?;
    let mut reader = Reader {
        webview: None,
        window: None,
        url,
        proxy: event_loop.create_proxy(),
        result: None,
    };
    event_loop.run_app(&mut reader)?;
    match reader.result.unwrap_or(Ok(None))? {
        Some(message) => {
            std::fs::write(output, message)?;
            Ok(true)
        }
        None => Ok(false),
    }
}
#[cfg(not(windows))]
pub fn open_reader(_: &str, _: &Path) -> Result<bool> {
    bail!("X 帖子窗口目前支持 Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    const URL: &str = "https://x.com/NASA/status/2018361967644127646";
    fn fixture() -> Value {
        serde_json::json!({"id_str":"2018361967644127646","text":"首段正文\n\n末尾完整内容","user":{"name":"NASA"},"created_at":"2026-02-02T16:32:47Z","mediaDetails":[{"type":"photo","media_url_https":"https://pbs.twimg.com/media/first.jpg"},{"type":"video"},{"type":"photo","media_url_https":"https://pbs.twimg.com/media/second.png"}],"quoted_tweet":{"text":"不属于当前帖子","mediaDetails":[{"type":"photo","media_url_https":"https://pbs.twimg.com/media/quote.jpg"}]}})
    }
    #[test]
    fn urls_are_official_statuses_and_canonical() {
        assert_eq!(
            link("分享：https://mobile.twitter.com/NASA/status/2018361967644127646/photo/1?s=20。"),
            link(URL)
        );
        assert_eq!(
            link("http://x.com/i/web/status/2018361967644127646"),
            link(URL)
        );
        for u in [
            "https://x.com.evil.test/NASA/status/123",
            "https://x.com@evil.test/NASA/status/123",
            "https://user@x.com/NASA/status/123",
            "https://x.com:444/NASA/status/123",
            "https://x.com/NASA",
            "https://x.com/NASA/status/123/likes",
            "https://x.com/NASA/status/123evil",
            "https://x.com/NASA/status/0",
        ] {
            assert!(link(u).is_none(), "{u}");
        }
    }
    #[test]
    fn ordered_own_images_and_original_text_are_retained() {
        let post = syndication_post(&fixture(), URL).unwrap();
        assert_eq!(post.images.len(), 2);
        assert!(post.images[0].contains("first"));
        assert!(post.images[1].contains("second"));
        assert_eq!(post.video_count, 1);
        assert!(post.body.ends_with("末尾完整内容"));
        assert!(!post.body.contains("不属于"));
        assert!(post.warning.is_some());
        let mut value = fixture();
        value["truncated"] = true.into();
        assert!(syndication_post(&value, URL).is_err());
        value = fixture();
        value["id_str"] = "123".into();
        assert!(syndication_post(&value, URL).is_err());
        assert!(photo_url("https://pbs.twimg.com.evil.test/media/x.jpg").is_none());
        assert!(photo_url("https://pbs.twimg.com/profile_images/x.jpg").is_none());
        assert!(image_extension(b"<html>denied</html>").is_none());
    }
    #[test]
    fn cookies_keep_x_only_and_preserve_httponly() {
        let input="# Netscape HTTP Cookie File\n#HttpOnly_.x.com\tTRUE\t/\tTRUE\t1999999999\tauth_token\ttest-only-value\n.twitter.com\tTRUE\t/\tTRUE\t1999999999\tct0\ttest-only-value\n.example.com\tTRUE\t/\tTRUE\t1999999999\tsecret\tshould-drop\n.x.com.evil.test\tTRUE\t/\tTRUE\t1999999999\tsecret\tshould-drop\n";
        let kept = cookie_contents(input).unwrap();
        assert!(kept.contains("#HttpOnly_.x.com"));
        assert!(kept.contains(".twitter.com"));
        assert!(!kept.contains("should-drop"));
        assert!(cookie_contents("not cookies").is_err());
    }
    #[test]
    fn browser_ipc_is_bound_to_requested_post() {
        let value=serde_json::json!({"kind":"twitter-post","source":URL,"body":"短文","author":"NASA","complete":true,"images":[],"video_count":0}).to_string();
        assert!(valid_ipc_source(&value, URL, URL));
        assert!(!valid_ipc_source(&value, "https://evil.test", URL));
        assert!(!valid_ipc_source(
            &value,
            "https://x.com/NASA/status/123",
            URL
        ));
        let mut incomplete: Value = serde_json::from_str(&value).unwrap();
        incomplete["complete"] = false.into();
        assert!(Post::from_message(&incomplete.to_string(), URL).is_err());
    }
    #[test]
    fn text_export_retains_metadata_and_tail() {
        let mut post = syndication_post(&fixture(), URL).unwrap();
        post.images.clear();
        let temp = tempfile::tempdir().unwrap();
        let folder = post
            .export(temp.path(), crate::export::TextFormat::Markdown)
            .unwrap();
        let text = std::fs::read_to_string(folder.join("帖子正文.md")).unwrap();
        for expected in ["NASA", "末尾完整内容", "2026-02-02", "2018361967644127646"] {
            assert!(text.contains(expected));
        }
    }

    #[test]
    #[ignore = "public X network QA: downloads one image and a 3-second native video"]
    fn live_public_photo_and_short_video() {
        let tool = crate::external::find_yt_dlp().expect("bundled yt-dlp");
        let output = Path::new("output/twitter-qa");
        let photo = probe(URL, &tool).unwrap();
        assert_eq!(photo.author, "NASA");
        assert_eq!(photo.images.len(), 1);
        assert!(photo.body.contains("wet dress rehearsal"));
        let package = photo
            .export(output, crate::export::TextFormat::Markdown)
            .unwrap();
        assert!(package.join("帖子正文.md").is_file());
        let image = std::fs::read_dir(&package)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("配图")
            })
            .unwrap();
        assert!(image_extension(&std::fs::read(image).unwrap()).is_some());
        let video = probe(
            "https://x.com/CaptainAmerica/status/719944021058060289",
            &tool,
        )
        .unwrap();
        assert_eq!(video.video_count, 1);
        let files = download(&video, &tool, output, &|_| {}).unwrap();
        assert_eq!(files.len(), 1);
        let mut probe_command = Command::new(tool.parent().unwrap().join("ffprobe.exe"));
        probe_command
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration:stream=codec_type,width,height",
                "-of",
                "json",
            ])
            .arg(&files[0]);
        let probe_output =
            run_bounded(&mut probe_command, Duration::from_secs(20), &|_| {}).unwrap();
        assert!(probe_output.status.success());
        let info: Value = serde_json::from_slice(&probe_output.stdout).unwrap();
        assert!(info["streams"]
            .as_array()
            .unwrap()
            .iter()
            .any(|stream| stream["codec_type"] == "video"
                && stream["width"].as_u64().unwrap_or(0) > 0));
        assert!(
            info["format"]["duration"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
                > 3.0
        );
    }
}
