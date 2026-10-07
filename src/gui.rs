use crate::bili::{
    lines_to_srt, lines_to_txt, sanitize_filename, Client, QrPoll, SubTrack, VideoInfo, VideoStream,
};
use crate::cli::output_dir;
use crate::export::{self, TextFormat};
use crate::external;
use crate::transcribe;
use crate::zhihu;
use anyhow::Result;
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

const LOGO_PNG: &[u8] = include_bytes!("../assets/subtitle-extractor-logo.png");

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Youtube,
    BiliText,
    Subtitle,
    Video,
    Transcribe,
    WebText,
    Douyin,
    Xhs,
    WechatArticle,
    WechatChannels,
}

enum Msg {
    WechatArticleLoaded(Result<Option<crate::wechat_article::Article>>),
    WechatVideoLoaded(Result<Option<crate::wechat_channels::Video>>),
    WechatSaved(Result<PathBuf>),
    CommentsLoaded(Result<Option<crate::comments::Comments>>),
    CommentsSaved(Result<PathBuf>),
    YoutubeLoaded(Result<crate::youtube::Video>),
    YoutubeSaved(Result<Vec<PathBuf>>),
    VideoLoaded(
        Box<VideoInfo>,
        usize,
        Vec<SubTrack>,
        Vec<VideoStream>,
        Vec<String>,
    ),
    TracksLoaded(usize, Vec<SubTrack>, Vec<VideoStream>, Vec<String>),
    Saved(Result<String>),
    Failed(String),
    VideoStage(String),
    VideoProgress(f64),
    VideoSaved(Result<String>),
    ExternalLoaded(Box<external::ExternalVideo>),
    ExternalSaved(Result<PathBuf>),
    TranscribeSaved(Result<Vec<PathBuf>>),
    ZhihuSaved(Result<zhihu::ZhihuExport>),
    ZhihuLoaded(Result<zhihu::ZhihuContent>),
    ZhihuLoginFinished(Result<bool>),
    DouyinLoginFinished(Result<bool>),
    XhsLoginFinished(Result<bool>),
    DouyinArticleLoaded(Result<Option<crate::douyin_article::Article>>),
    DouyinArticleSaved(Result<PathBuf>),
    QrReady {
        w: usize,
        pixels: Vec<egui::Color32>,
    },
    QrStage(String),
    QrConfirmed,
}

struct App {
    wechat_article: Option<Arc<crate::wechat_article::Article>>,
    wechat_video: Option<crate::wechat_channels::Video>,
    wechat_preview_open: bool,
    comments: Option<Arc<crate::comments::Comments>>,
    comment_selected: Vec<bool>,
    comment_window_open: bool,
    comment_page: usize,
    comment_filter: String,
    comment_sort: crate::comments::Sort,
    logo: egui::TextureHandle,
    ffmpeg_ok: bool,
    mode: Mode,
    youtube: Option<crate::youtube::Video>,
    youtube_subtitles: bool,
    youtube_track: usize,
    link: String,
    status: String,
    busy: bool,
    video: Option<VideoInfo>,
    selected_page: usize,
    tracks: Vec<SubTrack>,
    selected_track: usize,
    streams: Vec<VideoStream>,
    selected_stream: usize,
    video_progress: Option<f64>,
    external: Option<external::ExternalVideo>,
    douyin_article_mode: bool,
    gallery_mode: bool,
    xhs_video: bool,
    zhihu_article: bool,
    douyin_article: Option<Arc<crate::douyin_article::Article>>,
    douyin_preview_open: bool,
    media_file: Option<PathBuf>,
    text_format: TextFormat,
    saved_files: Vec<PathBuf>,
    reveal_result: bool,
    transcribe_language: String,
    speaker_diarization: bool,
    speaker_count: usize,
    bili_local_transcribe: bool,
    zhihu_from: String,
    zhihu_to: String,
    zhihu_all_answers: bool,
    zhihu_content: Option<Arc<zhihu::ZhihuContent>>,
    output_dir: PathBuf,
    output_settings_open: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    qr_texture: Option<egui::TextureHandle>,
    qr_stage: Option<String>,
    qr_cancelled: Arc<AtomicBool>,
    logged_in: bool,
}

impl App {
    fn spawn_comments(&mut self) {
        let Some(input) = crate::comments::link(&self.link) else {
            self.status = "请先粘贴支持平台的内容链接，再检测评论".into();
            return;
        };
        self.busy = true;
        self.status = "请在官方窗口打开评论区，检测后返回勾选评论".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = (|| -> Result<Option<crate::comments::Comments>> {
                let response = tempfile::NamedTempFile::new()?;
                let status = std::process::Command::new(std::env::current_exe()?)
                    .arg("--comments-reader")
                    .arg(&input)
                    .arg(response.path())
                    .status()?;
                match status.code() {
                    Some(0) => Ok(Some(crate::comments::Comments::from_message(
                        &std::fs::read_to_string(response.path())?,
                        &input,
                    )?)),
                    Some(2) => Ok(None),
                    _ => anyhow::bail!("评论窗口未正常完成读取，请重试"),
                }
            })();
            let _ = tx.send(Msg::CommentsLoaded(result));
        });
    }
    fn export_comments(&mut self) {
        let Some(comments) = self.comments.clone() else {
            return;
        };
        let selected = self.comment_selected.clone();
        if !selected.iter().any(|value| *value) {
            self.status = "请先勾选要导出的评论".into();
            return;
        }
        let directory = self.output_dir.clone();
        let format = self.text_format;
        let tx = self.tx.clone();
        self.busy = true;
        self.status = "正在导出所选评论…".into();
        let sort = self.comment_sort;
        thread::spawn(move || {
            let _ = tx.send(Msg::CommentsSaved(
                comments.export(&selected, &directory, format, sort),
            ));
        });
    }
    fn spawn_xhs_login(&mut self) {
        self.busy = true;
        self.status = "请在小红书官方窗口登录，完成后关闭窗口，再获取图文".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = (|| -> Result<bool> {
                let output = tempfile::NamedTempFile::new()?;
                let mut command = std::process::Command::new(std::env::current_exe()?);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let status = command
                    .arg("--xhs-gallery")
                    .arg("https://www.xiaohongshu.com/explore")
                    .arg(output.path())
                    .status()?;
                if !matches!(status.code(), Some(0 | 2)) {
                    anyhow::bail!("无法打开小红书窗口");
                }
                Ok(true)
            })();
            let _ = tx.send(Msg::XhsLoginFinished(result));
        });
    }
    fn spawn_read_douyin_article(&mut self) {
        let valid = if self.mode == Mode::BiliText {
            crate::douyin_article::bili_url(&self.link).is_some()
        } else {
            external::is_douyin(&self.link)
        };
        if !valid {
            self.status = if self.mode == Mode::BiliText {
                "请粘贴 B站图文或专栏链接"
            } else {
                "请粘贴抖音长文章分享链接"
            }
            .into();
            return;
        }
        self.douyin_article = None;
        self.busy = true;
        self.status = "请在文章窗口阅读全文，点击右下角获取正文，再回到拾文确认导出".into();
        let input = self.link.clone();
        let tx = self.tx.clone();
        let bili = self.mode == Mode::BiliText;
        thread::spawn(move || {
            let result = (|| -> Result<Option<crate::douyin_article::Article>> {
                let response = tempfile::NamedTempFile::new()?;
                let status = std::process::Command::new(std::env::current_exe()?)
                    .arg(if bili {
                        "--bili-article"
                    } else {
                        "--douyin-article"
                    })
                    .arg(&input)
                    .arg(response.path())
                    .status()?;
                match status.code() {
                    Some(0) => Ok(Some(crate::douyin_article::Article::from_message(
                        &std::fs::read_to_string(response.path())?,
                    )?)),
                    Some(2) => Ok(None),
                    _ => anyhow::bail!("文章窗口未能完成获取，请重试"),
                }
            })();
            let _ = tx.send(Msg::DouyinArticleLoaded(result));
        });
    }
    fn spawn_export_douyin_article(&mut self) {
        let Some(article) = self.douyin_article.clone() else {
            return;
        };
        let dir = self.output_dir.clone();
        let format = self.text_format;
        self.busy = true;
        self.status = "正在导出文章…".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(Msg::DouyinArticleSaved(article.export(&dir, format)));
        });
    }
    fn spawn_douyin_login(&mut self) {
        self.busy = true;
        self.external = None;
        self.status = "请在抖音官方窗口访问或登录，完成后关闭窗口，再重新获取视频".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = (|| -> Result<bool> {
                let status = std::process::Command::new(std::env::current_exe()?)
                    .arg("--douyin-login")
                    .status()?;
                match status.code() {
                    Some(0) => Ok(true),
                    Some(2) => Ok(false),
                    _ => anyhow::bail!("抖音登录窗口未能完成，请重试"),
                }
            })();
            let _ = tx.send(Msg::DouyinLoginFinished(result));
        });
    }
    fn spawn_zhihu_login(&mut self) {
        self.busy = true;
        self.zhihu_content = None;
        self.status = "请在知乎官方窗口登录；完成后关闭登录窗口，再重新获取文章".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = (|| -> Result<bool> {
                let status = std::process::Command::new(std::env::current_exe()?)
                    .arg("--zhihu-login")
                    .status()?;
                match status.code() {
                    Some(0) => Ok(true),
                    Some(2) => Ok(false),
                    _ => anyhow::bail!("未能完成知乎登录，请重试"),
                }
            })();
            let _ = tx.send(Msg::ZhihuLoginFinished(result));
        });
    }
    fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let default_output = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("字幕输出")))
            .unwrap_or_else(output_dir);
        let icon = eframe::icon_data::from_png_bytes(LOGO_PNG).expect("Invalid application logo");
        let logo = ctx.load_texture(
            "app-logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        Self {
            wechat_article: None,
            wechat_video: None,
            wechat_preview_open: false,
            comments: None,
            comment_selected: Vec::new(),
            comment_window_open: false,
            comment_page: 0,
            comment_filter: String::new(),
            comment_sort: crate::comments::Sort::Original,
            logo,
            mode: Mode::Subtitle,
            youtube: None,
            youtube_subtitles: false,
            youtube_track: 0,
            ffmpeg_ok: Client::ffmpeg_available(),
            link: String::new(),
            status: "粘贴B站视频链接，然后点「获取」".into(),
            busy: false,
            video: None,
            selected_page: 1,
            tracks: Vec::new(),
            selected_track: 0,
            streams: Vec::new(),
            selected_stream: 0,
            video_progress: None,
            external: None,
            douyin_article_mode: false,
            gallery_mode: false,
            xhs_video: false,
            zhihu_article: false,
            douyin_article: None,
            douyin_preview_open: false,
            media_file: None,
            text_format: TextFormat::Txt,
            saved_files: recent_saved_file(&default_output).into_iter().collect(),
            reveal_result: false,
            transcribe_language: "auto".into(),
            speaker_diarization: true,
            speaker_count: 0,
            bili_local_transcribe: false,
            zhihu_from: String::new(),
            zhihu_to: String::new(),
            zhihu_all_answers: true,
            zhihu_content: None,
            output_dir: default_output,
            output_settings_open: false,
            tx,
            rx,
            qr_texture: None,
            qr_stage: None,
            qr_cancelled: Arc::new(AtomicBool::new(false)),
            logged_in: Client::has_saved_cookies(),
        }
    }

    fn spawn_export_youtube(&mut self) {
        let Some(video) = self.youtube.clone() else {
            return;
        };
        let Some(track) = video.tracks.get(self.youtube_track).cloned() else {
            self.status = "该视频没有可导出的字幕，可下载后使用本地转写".into();
            return;
        };
        let Some(tool) = external::find_yt_dlp() else {
            self.status = "下载组件未就绪，请重新安装应用".into();
            return;
        };
        let directory = self.output_dir.clone();
        let format = self.text_format;
        let tx = self.tx.clone();
        self.busy = true;
        self.status = "正在获取并导出 YouTube 字幕…".into();
        thread::spawn(move || {
            let _ = tx.send(Msg::YoutubeSaved(crate::youtube::export(
                &video, &track, &tool, &directory, format,
            )));
        });
    }

    fn spawn_fetch_video(&mut self) {
        let input = self.link.clone();
        if input.trim().is_empty() {
            self.status = "请先粘贴链接".into();
            return;
        }
        self.external = None;
        self.video = None;
        self.tracks.clear();
        self.streams.clear();
        self.video_progress = None;
        self.busy = true;

        if self.mode == Mode::Youtube {
            self.youtube = None;
            if crate::youtube::link(&input).is_none() {
                self.busy = false;
                self.status = "请粘贴 YouTube 视频、Shorts 或短链接".into();
                return;
            }
            let Some(tool) = external::find_yt_dlp() else {
                self.busy = false;
                self.status = "下载组件未就绪，请重新安装应用".into();
                return;
            };
            self.status = "正在获取 YouTube 视频与字幕语言…".into();
            let tx = self.tx.clone();
            thread::spawn(move || {
                let _ = tx.send(Msg::YoutubeLoaded(crate::youtube::probe(&input, &tool)));
            });
            return;
        }
        if self.mode == Mode::Xhs && !self.xhs_video {
            if !external::is_xhs(&input) {
                self.busy = false;
                self.status = "请粘贴小红书图文链接或整段分享文字".into();
                return;
            }
            self.status = "正在打开小红书，读取图文和图片数量...".into();
            let tx = self.tx.clone();
            thread::spawn(move || match external::probe_xhs(&input) {
                Ok(video) => {
                    let _ = tx.send(Msg::ExternalLoaded(Box::new(video)));
                }
                Err(error) => {
                    let _ = tx.send(Msg::Failed(format!("小红书图文获取失败：{error:#}")));
                }
            });
            return;
        }
        if self.mode == Mode::Douyin && !external::is_douyin(&input) {
            self.busy = false;
            self.status = "请粘贴抖音视频链接、精选链接或分享文字".into();
            return;
        }
        if self.mode == Mode::Xhs && !external::is_xhs(&input) {
            self.busy = false;
            self.status = "请粘贴小红书链接".into();
            return;
        }
        if self.mode == Mode::Video && external::is_supported(&input) {
            self.busy = false;
            self.status = "请在对应的抖音或小红书页面获取".into();
            return;
        }
        if external::is_supported(&input) {
            if !matches!(self.mode, Mode::Video | Mode::Douyin | Mode::Xhs) {
                self.busy = false;
                self.status = "请切换到对应的「抖音」或「小红书」页面，再选择视频或图文。".into();
                return;
            }
            let Some(tool) = external::find_yt_dlp() else {
                self.busy = false;
                self.status = "未找到 tools/yt-dlp.exe".into();
                return;
            };
            self.status = "正在解析抖音/小红书链接...".into();
            let tx = self.tx.clone();
            thread::spawn(move || match external::probe(&input, &tool) {
                Ok(video) => {
                    let _ = tx.send(Msg::ExternalLoaded(Box::new(video)));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(format!("解析链接失败: {e:#}")));
                }
            });
            return;
        }

        self.tracks.clear();
        self.streams.clear();
        self.selected_track = 0;
        self.selected_stream = 0;
        self.status = "正在获取视频信息和画质列表...".into();
        let tx = self.tx.clone();
        let local_audio = self.mode == Mode::Subtitle && self.bili_local_transcribe;
        thread::spawn(move || {
            let client = Client::new();
            match client.fetch_video(&input) {
                Ok((video, page)) => {
                    let (tracks, streams, errors) = if local_audio {
                        match client.fetch_streams(&video, page) {
                            Ok(streams) => (vec![], streams, vec![]),
                            Err(error) => {
                                (vec![], vec![], vec![format!("获取音频信息失败：{error:#}")])
                            }
                        }
                    } else {
                        load_media_options(&client, &video, page)
                    };
                    let _ = tx.send(Msg::VideoLoaded(
                        Box::new(video),
                        page,
                        tracks,
                        streams,
                        errors,
                    ));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(format!("获取视频信息失败: {e:#}")));
                }
            }
        });
    }

    fn spawn_fetch_tracks(&mut self, page: usize) {
        let Some(video) = self.video.as_ref() else {
            return;
        };
        let video: VideoInfo = VideoInfo {
            aid: video.aid,
            bvid: video.bvid.clone(),
            title: video.title.clone(),
            pages: video
                .pages
                .iter()
                .map(|p| crate::bili::PageInfo {
                    page: p.page,
                    part: p.part.clone(),
                    cid: p.cid,
                })
                .collect(),
        };
        self.busy = true;
        self.status = format!("正在获取第 {page} 个分P的信息...");
        self.tracks.clear();
        self.streams.clear();
        self.selected_track = 0;
        self.selected_stream = 0;
        let tx = self.tx.clone();
        let local_audio = self.mode == Mode::Subtitle && self.bili_local_transcribe;
        thread::spawn(move || {
            let client = Client::new();
            let (tracks, streams, errors) = if local_audio {
                match client.fetch_streams(&video, page) {
                    Ok(streams) => (vec![], streams, vec![]),
                    Err(error) => (vec![], vec![], vec![format!("获取音频信息失败：{error:#}")]),
                }
            } else {
                load_media_options(&client, &video, page)
            };
            let _ = tx.send(Msg::TracksLoaded(page, tracks, streams, errors));
        });
    }

    fn spawn_download(&mut self, fmt: &str) {
        let Some(video) = self.video.as_ref() else {
            return;
        };
        let Some(track) = self.tracks.get(self.selected_track) else {
            return;
        };
        let url = track.url.clone();
        let lan = track.lan.clone();
        let lan_doc = track.lan_doc.clone();
        let title = video.title.clone();
        let page = self.selected_page;
        let fmt = fmt.to_string();
        let text_format = self.text_format;
        let dir = self.output_dir.clone();
        let tx = self.tx.clone();

        self.busy = true;
        self.status = format!("正在下载 {lan_doc} 字幕...");
        thread::spawn(move || {
            let client = Client::new();
            let res = (|| -> Result<String> {
                let lines = client.fetch_subtitle_body(&url)?;
                let content = if fmt == "txt" {
                    lines_to_txt(&lines)
                } else {
                    lines_to_srt(&lines)
                };
                let ext = if fmt == "txt" { "txt" } else { "srt" };
                let mut name = sanitize_filename(&title);
                if page > 1 {
                    name.push_str(&format!("_P{page}"));
                }
                name.push_str(&format!("_{lan}.{ext}"));
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(&name);
                std::fs::write(&path, content)?;
                let path = if fmt == "txt" {
                    export::convert(&path, text_format)?
                } else {
                    path
                };
                Ok(path.display().to_string())
            })();
            let _ = tx.send(Msg::Saved(res));
        });
    }
    fn spawn_download_video(&mut self) {
        let Some(video) = self.video.as_ref() else {
            return;
        };
        let Some(stream) = self.streams.get(self.selected_stream) else {
            return;
        };
        let stream = stream.clone();
        let title = video.title.clone();
        let page = self.selected_page;
        let dir = self.output_dir.clone();
        let tx = self.tx.clone();

        self.busy = true;
        self.video_progress = Some(0.0);
        self.status = "正在下载视频画面...".into();
        thread::spawn(move || {
            let client = Client::new();
            let res = (|| -> Result<String> {
                let mut name = sanitize_filename(&title);
                if page > 1 {
                    name.push_str(&format!("_P{page}"));
                }
                name.push_str(&format!("_{}.mp4", stream.quality_id));
                let out_path = client.download_video_stream(
                    &stream,
                    &dir,
                    &name,
                    &|p| {
                        let _ = tx.send(Msg::VideoProgress(p));
                    },
                    &|stage| {
                        let _ = tx.send(Msg::VideoStage(stage.into()));
                        let _ = tx.send(Msg::VideoProgress(0.0));
                    },
                )?;
                Ok(out_path.display().to_string())
            })();
            let _ = tx.send(Msg::VideoSaved(res));
        });
    }

    fn spawn_download_external(&mut self) {
        let Some(external_video) = self.external.clone() else {
            return;
        };
        let tool = external::find_yt_dlp();
        if external_video.gallery.is_none() && tool.is_none() {
            self.status = "未找到 tools/yt-dlp.exe".into();
            return;
        }
        let dir = self.output_dir.clone();
        let tx = self.tx.clone();

        self.busy = true;
        self.video_progress = Some(0.0);
        let format = self.text_format;
        self.status = if external_video.gallery.is_some() {
            "正在保存图文图片和文案..."
        } else {
            "正在下载视频..."
        }
        .into();
        thread::spawn(move || {
            let progress = |p| {
                let _ = tx.send(Msg::VideoProgress(p));
            };
            let res = if external_video.gallery.is_some() {
                external::download_gallery(&external_video, &dir, format, &progress)
            } else {
                external::download(
                    &external_video.url,
                    tool.as_deref().expect("video tool checked"),
                    &dir,
                    &progress,
                )
            };
            let _ = tx.send(Msg::ExternalSaved(res));
        });
    }

    fn spawn_transcribe(&mut self) {
        let Some(media) = self.media_file.clone() else {
            self.status = "请先选择音频或视频文件".into();
            return;
        };
        let Some(model) = transcribe::find_default_model() else {
            self.status = "内置识别资源缺失，请重新安装拾文".into();
            return;
        };
        let language = self.transcribe_language.clone();
        let dir = self.output_dir.clone();
        let tx = self.tx.clone();

        self.busy = true;
        self.status = "正在转换音频并识别文字...".into();
        let text_format = self.text_format;
        let options = transcribe::SpeakerOptions {
            enabled: self.speaker_diarization,
            count: self.speaker_count,
        };
        thread::spawn(move || {
            let res = transcribe::run(&media, &model, &language, &dir, options, &|s| {
                let _ = tx.send(Msg::VideoStage(s.into()));
            })
            .and_then(|paths| {
                paths
                    .into_iter()
                    .map(|path| {
                        if path.extension().and_then(|ext| ext.to_str()) == Some("txt") {
                            export::convert(&path, text_format)
                        } else {
                            Ok(path)
                        }
                    })
                    .collect()
            });
            let _ = tx.send(Msg::TranscribeSaved(res));
        });
    }

    fn spawn_bili_transcribe(&mut self) {
        let Some(video) = self.video.clone() else {
            return;
        };
        let page = self.selected_page;
        let language = self.transcribe_language.clone();
        let dir = self.output_dir.clone();
        let format = self.text_format;
        let options = transcribe::SpeakerOptions {
            enabled: self.speaker_diarization,
            count: self.speaker_count,
        };
        let tx = self.tx.clone();
        self.busy = true;
        self.video_progress = None;
        self.status = "正在准备音频本地转写…".into();
        thread::spawn(move || {
            let result = transcribe::run_bili(
                &video,
                page,
                &language,
                &dir,
                options,
                &|s| {
                    let _ = tx.send(Msg::VideoStage(s.into()));
                },
                &|p| {
                    let _ = tx.send(Msg::VideoProgress(p));
                },
            )
            .and_then(|paths| {
                paths
                    .into_iter()
                    .map(|path| {
                        if path.extension().and_then(|s| s.to_str()) == Some("txt") {
                            export::convert(&path, format)
                        } else {
                            Ok(path)
                        }
                    })
                    .collect()
            });
            let _ = tx.send(Msg::TranscribeSaved(result));
        });
    }

    fn spawn_fetch_zhihu(&mut self) {
        let input = self.link.trim().to_string();
        if input.is_empty() {
            self.status = "请先粘贴知乎回答或专栏链接".into();
            return;
        }
        if let Ok(url) = reqwest::Url::parse(&input) {
            let article = url.path().starts_with("/p/");
            if article != self.zhihu_article {
                self.status = if self.zhihu_article {
                    "请粘贴知乎专栏文章链接，问答请切换到「问答」"
                } else {
                    "请粘贴知乎问题或回答链接，专栏请切换到「专栏」"
                }
                .into();
                return;
            }
        }
        self.zhihu_content = None;
        self.zhihu_from.clear();
        self.zhihu_to.clear();
        let tx = self.tx.clone();
        let all_answers = self.zhihu_all_answers;
        self.busy = true;
        self.status = "正在获取知乎内容与回答列表…".into();
        thread::spawn(move || {
            let _ = tx.send(Msg::ZhihuLoaded(zhihu::fetch_content(&input, all_answers)));
        });
    }

    fn spawn_export_zhihu(&mut self) {
        let Some(content) = self.zhihu_content.clone() else {
            return;
        };
        let parse = |text: &str| -> Result<Option<usize>> {
            if text.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(
                    text.trim()
                        .parse::<usize>()
                        .map_err(|_| anyhow::anyhow!("回答序号必须为正整数"))?,
                ))
            }
        };
        let bounds = parse(&self.zhihu_from).and_then(|from| Ok((from, parse(&self.zhihu_to)?)));
        let (from, to) = match bounds {
            Ok(bounds) => bounds,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let first = from.unwrap_or(1);
        let last = to.unwrap_or(content.count());
        if first == 0 || last == 0 || first > last || last > content.count() {
            self.status = format!(
                "导出范围必须在 1-{} 之间，且起始序号不能大于结束序号",
                content.count()
            );
            return;
        }
        let dir = self.output_dir.clone();
        let format = self.text_format;
        let tx = self.tx.clone();
        self.busy = true;
        self.status = "正在导出已获取的内容…".into();
        thread::spawn(move || {
            let _ = tx.send(Msg::ZhihuSaved(content.export(&dir, from, to, format)));
        });
    }

    fn spawn_qr_login(&mut self) {
        self.qr_cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.qr_cancelled.clone();
        self.busy = true;
        self.status = "正在生成登录二维码...".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let client = Client::new();
            let qr = match client.qr_generate() {
                Ok(q) => q,
                Err(e) => {
                    let _ = tx.send(Msg::Failed(format!("生成二维码失败: {e:#}")));
                    return;
                }
            };
            let code = match qrcode::QrCode::new(qr.url.as_bytes()) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Msg::Failed(format!("生成二维码失败: {e}")));
                    return;
                }
            };
            let w = code.width();
            let pixels: Vec<egui::Color32> = code
                .to_colors()
                .into_iter()
                .map(|c| {
                    if c == qrcode::Color::Dark {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::WHITE
                    }
                })
                .collect();
            let _ = tx.send(Msg::QrReady { w, pixels });
            for _ in 0..60 {
                std::thread::sleep(std::time::Duration::from_millis(2000));
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let poll = client.qr_poll(&qr.qrcode_key);
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                match poll {
                    Ok(QrPoll::Waiting) => {
                        let _ = tx.send(Msg::QrStage("等待扫码...".into()));
                    }
                    Ok(QrPoll::Scanned) => {
                        let _ = tx.send(Msg::QrStage("已扫码，请在手机上确认".into()));
                    }
                    Ok(QrPoll::Confirmed) => {
                        let _ = tx.send(Msg::QrConfirmed);
                        return;
                    }
                    Ok(QrPoll::Expired) => {
                        let _ = tx.send(Msg::Failed("二维码已过期，请重新点击「扫码登录」".into()));
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::Failed(format!("登录状态查询失败: {e:#}")));
                        return;
                    }
                }
            }
            if !cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(Msg::Failed("登录超时，请重新扫码".into()));
            }
        });
    }

    fn poll_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            let previous_status = self.status.clone();
            if self.qr_cancelled.load(Ordering::Relaxed)
                && matches!(
                    &msg,
                    Msg::QrReady { .. } | Msg::QrStage(_) | Msg::QrConfirmed
                )
            {
                continue;
            }
            if !matches!(
                &msg,
                Msg::VideoStage(_) | Msg::VideoProgress(_) | Msg::QrStage(_) | Msg::QrReady { .. }
            ) {
                self.busy = false;
            }
            match msg {
                Msg::WechatArticleLoaded(result) => match result {
                    Ok(Some(article)) => {
                        self.status = format!(
                            "已获取公众号正文 {} 字、{} 张配图，核对后确认导出",
                            article.body.chars().count(),
                            article.images.len()
                        );
                        self.wechat_article = Some(Arc::new(article));
                    }
                    Ok(None) => self.status = "已关闭公众号窗口，未导出文件".into(),
                    Err(error) => self.status = format!("公众号获取失败：{error:#}"),
                },
                Msg::WechatVideoLoaded(result) => match result {
                    Ok(Some(video)) => {
                        self.status =
                            "已检测到网页视频，确认后下载；保存前会检查文件能否播放".into();
                        self.wechat_video = Some(video);
                    }
                    Ok(None) => self.status = "已关闭视频号窗口，未下载文件".into(),
                    Err(error) => self.status = format!("视频号获取失败：{error:#}"),
                },
                Msg::WechatSaved(result) => {
                    self.video_progress = None;
                    match result {
                        Ok(path) => {
                            self.status = format!("已保存: {}", path.display());
                            self.saved_files = vec![path];
                        }
                        Err(error) => self.status = format!("微信内容保存失败：{error:#}"),
                    }
                }
                Msg::CommentsLoaded(result) => match result {
                    Ok(Some(comments)) => {
                        self.status =
                            format!("已检测 {} 条已加载评论，请勾选后导出", comments.items.len());
                        self.comment_selected = vec![false; comments.items.len()];
                        self.comments = Some(Arc::new(comments));
                        self.comment_page = 0;
                        self.comment_filter.clear();
                        self.comment_sort = crate::comments::Sort::Original;
                        self.comment_window_open = true;
                    }
                    Ok(None) => self.status = "已取消评论检测，未导出文件".into(),
                    Err(error) => self.status = format!("评论检测失败：{error:#}"),
                },
                Msg::CommentsSaved(result) => match result {
                    Ok(path) => {
                        self.saved_files = vec![path];
                        self.status = "所选评论已导出，可在左侧打开文件".into();
                    }
                    Err(error) => self.status = format!("评论导出失败：{error:#}"),
                },
                Msg::YoutubeLoaded(result) => match result {
                    Ok(video) => {
                        self.youtube_track = 0;
                        self.status = format!("视频已获取，可用字幕 {} 种", video.tracks.len());
                        self.external = Some(external::ExternalVideo {
                            title: video.title.clone(),
                            url: video.url.clone(),
                            gallery: None,
                        });
                        self.youtube = Some(video);
                    }
                    Err(error) => self.status = format!("{error:#}"),
                },
                Msg::YoutubeSaved(result) => match result {
                    Ok(paths) => {
                        self.saved_files = paths;
                        self.status = "YouTube 字幕导出完成".into();
                    }
                    Err(error) => self.status = format!("{error:#}"),
                },
                Msg::DouyinArticleLoaded(result) => match result {
                    Ok(Some(article)) => {
                        self.status = format!(
                            "已获取文字 {} 字，请核对正文与末尾后确认导出{}",
                            article.body.chars().count(),
                            if article.selected {
                                "（当前为选中文字）"
                            } else {
                                ""
                            }
                        );
                        self.douyin_article = Some(Arc::new(article));
                    }
                    Ok(None) => self.status = "已取消获取文章".into(),
                    Err(error) => self.status = format!("文章获取失败：{error:#}"),
                },
                Msg::DouyinArticleSaved(result) => match result {
                    Ok(path) => {
                        self.saved_files = vec![path.clone()];
                        self.status = format!("已保存: {}", path.display());
                    }
                    Err(error) => self.status = format!("文章导出失败：{error:#}"),
                },
                Msg::DouyinLoginFinished(result) => {
                    self.busy = false;
                    self.status = match result {
                        Ok(true) => "抖音访问会话已保存，请重新获取视频".into(),
                        Ok(false) => "未获取抖音访问会话，请重试".into(),
                        Err(error) => format!("抖音登录失败：{error:#}"),
                    };
                }
                Msg::XhsLoginFinished(result) => {
                    self.busy = false;
                    self.status = match result {
                        Ok(_) => "小红书窗口已关闭，请粘贴笔记分享文字并获取图文".into(),
                        Err(error) => format!("小红书窗口打开失败：{error:#}"),
                    };
                }
                Msg::ExternalLoaded(video) => {
                    self.video = None;
                    self.tracks.clear();
                    self.streams.clear();
                    self.status = match &video.gallery {
                        Some(gallery) => format!(
                            "已获取图文作品：{} 张图片，可保存图片和文案",
                            gallery.images.len()
                        ),
                        None => "已解析链接，可以下载视频".into(),
                    };
                    if self.mode == Mode::Douyin {
                        self.gallery_mode = video.gallery.is_some();
                    }
                    self.external = Some(*video);
                }
                Msg::ExternalSaved(res) => match res {
                    Ok(path) => {
                        self.saved_files = vec![path.clone()];
                        self.video_progress = None;
                        self.status = format!("已保存: {}", path.display());
                    }
                    Err(e) => {
                        self.video_progress = None;
                        self.status = format!("下载失败: {e:#}");
                    }
                },
                Msg::VideoLoaded(video, page, tracks, streams, errors) => {
                    self.selected_page = video
                        .pages
                        .iter()
                        .find(|p| p.page == page)
                        .map(|p| p.page)
                        .unwrap_or_else(|| video.pages[0].page);
                    self.video = Some(*video);
                    self.tracks = tracks;
                    self.selected_track = 0;
                    self.streams = streams;
                    self.selected_stream = 0;
                    if self.mode == Mode::Subtitle
                        && self.bili_local_transcribe
                        && errors.is_empty()
                    {
                        self.status = "已获取视频，可选择分P并下载音频本地转写".into();
                    } else if !errors.is_empty() {
                        self.status = errors.join("；");
                    } else if self.mode == Mode::Video && self.streams.is_empty() {
                        self.status = "获取到视频信息，但没有可用画质".into();
                    } else if self.mode == Mode::Subtitle && self.tracks.is_empty() {
                        self.status = if self.logged_in {
                            "没有可下载的字幕".into()
                        } else {
                            "未登录：AI字幕需要登录才能获取".into()
                        };
                    } else {
                        self.status = format!(
                            "获取到 {} 条字幕、{} 个画质",
                            self.tracks.len(),
                            self.streams.len()
                        );
                    }
                }
                Msg::TracksLoaded(page, tracks, streams, errors) => {
                    self.selected_page = page;
                    self.tracks = tracks;
                    self.selected_track = 0;
                    self.streams = streams;
                    self.selected_stream = 0;
                    if !errors.is_empty() {
                        self.status = errors.join("；");
                    } else if self.mode == Mode::Subtitle && self.bili_local_transcribe {
                        self.status = "已获取当前分P音频信息，可下载音频本地转写".into();
                    } else if self.mode == Mode::Video && self.streams.is_empty() {
                        self.status = "该分P没有可用画质".into();
                    } else if self.mode == Mode::Subtitle && self.tracks.is_empty() {
                        self.status = "该分P没有可下载的字幕".into();
                    } else {
                        self.status = format!(
                            "获取到 {} 条字幕、{} 个画质",
                            self.tracks.len(),
                            self.streams.len()
                        );
                    }
                }
                Msg::VideoStage(s) => {
                    self.status = s;
                    self.video_progress = None;
                }
                Msg::VideoProgress(p) => self.video_progress = Some(p.clamp(0.0, 1.0)),
                Msg::ZhihuLoginFinished(result) => {
                    self.busy = false;
                    self.status = match result {
                        Ok(true) => "知乎已登录，请重新获取正文".into(),
                        Ok(false) => "知乎登录尚未完成，请重试".into(),
                        Err(error) => format!("知乎登录失败：{error:#}"),
                    };
                }
                Msg::ZhihuLoaded(result) => match result {
                    Ok(content) => {
                        self.zhihu_from = "1".into();
                        self.zhihu_to = content.count().to_string();
                        self.status = format!(
                            "已获取 {} 条{}，请选择范围后确认导出",
                            content.count(),
                            if content.is_question {
                                "回答"
                            } else {
                                "内容"
                            }
                        );
                        self.zhihu_content = Some(Arc::new(content));
                    }
                    Err(error) => self.status = format!("获取文字失败: {error:#}"),
                },
                Msg::ZhihuSaved(res) => match res {
                    Ok(path) => {
                        self.saved_files = vec![path.path.clone()];
                        self.busy = false;
                        self.status = format!("已保存: {}", path.display());
                    }
                    Err(e) => {
                        self.busy = false;
                        self.status = format!("获取文字失败: {e:#}");
                    }
                },
                Msg::TranscribeSaved(res) => match res {
                    Ok(paths) => {
                        self.video_progress = None;
                        self.saved_files = paths.clone();
                        let texts: Vec<String> =
                            paths.iter().map(|p| p.display().to_string()).collect();
                        self.status = format!("已保存: {}", texts.join(" 和 "));
                    }
                    Err(e) => {
                        self.video_progress = None;
                        self.status = format!("识别失败: {e:#}");
                    }
                },
                Msg::VideoSaved(res) => match res {
                    Ok(path) => {
                        self.saved_files = vec![PathBuf::from(&path)];
                        self.video_progress = None;
                        self.status = format!("已保存: {path}");
                    }
                    Err(e) => {
                        self.video_progress = None;
                        self.status = format!("下载失败: {e:#}");
                    }
                },
                Msg::Saved(res) => match res {
                    Ok(path) => {
                        self.saved_files = vec![PathBuf::from(&path)];
                        self.status = format!("已保存: {path}");
                    }
                    Err(e) => self.status = format!("保存失败: {e:#}"),
                },
                Msg::Failed(e) => {
                    self.status = e;
                    self.qr_stage = None;
                    self.qr_texture = None;
                }
                Msg::QrReady { w, pixels } => {
                    let image = egui::ColorImage {
                        size: [w, w],
                        pixels,
                    };
                    self.qr_texture =
                        Some(ctx.load_texture("qr", image, egui::TextureOptions::NEAREST));
                    self.qr_stage = Some("请用B站App扫一扫".into());
                }
                Msg::QrStage(s) => {
                    self.qr_stage = Some(s);
                }
                Msg::QrConfirmed => {
                    self.logged_in = true;
                    self.qr_stage = None;
                    self.qr_texture = None;
                    if self.video.is_some() {
                        self.status = "登录成功！正在重新获取字幕...".into();
                        self.spawn_fetch_video();
                    } else {
                        self.status = "登录成功！Cookie已保存，现在可以获取字幕了".into();
                    }
                }
            }
            if self.status != previous_status
                && (self.status.starts_with("已保存:") || self.status.contains("失败"))
            {
                self.reveal_result = true;
            }
        }
    }
}

fn load_media_options(
    client: &Client,
    video: &VideoInfo,
    page: usize,
) -> (Vec<SubTrack>, Vec<VideoStream>, Vec<String>) {
    let mut errors = Vec::new();
    let tracks = client.fetch_tracks(video, page).unwrap_or_else(|e| {
        errors.push(format!("获取字幕列表失败: {e:#}"));
        Vec::new()
    });
    let streams = client.fetch_streams(video, page).unwrap_or_else(|e| {
        errors.push(format!("获取画质列表失败: {e:#}"));
        Vec::new()
    });
    (tracks, streams, errors)
}

mod comments_ui;
mod layout;
mod reference;
mod wechat;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(35, 35, 35);

fn recent_saved_file(dir: &std::path::Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter(|entry| {
            matches!(
                entry.path().extension().and_then(|ext| ext.to_str()),
                Some("txt" | "srt" | "md" | "docx" | "pdf" | "mp4")
            )
        })
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        })
        .map(|entry| entry.path())
}

fn open_saved_file(path: &std::path::Path, select: bool) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("文件已被移动或删除");
    }
    let mut command = std::process::Command::new("explorer.exe");
    if select {
        let mut argument = std::ffi::OsString::from("/select,");
        argument.push(path);
        command.arg(argument);
    } else {
        command.arg(path);
    }
    command.spawn()?;
    Ok(())
}
const ACCENT_DARK: egui::Color32 = egui::Color32::from_rgb(45, 45, 45);

fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(label)
                .color(egui::Color32::WHITE)
                .strong(),
        )
        .fill(ACCENT)
        .stroke(egui::Stroke::NONE)
        .min_size(egui::vec2(128.0, 40.0))
        .rounding(10.0),
    )
    .clicked()
}

fn secondary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).size(12.0).color(ACCENT_DARK))
            .fill(egui::Color32::from_gray(245))
            .stroke(egui::Stroke::NONE)
            .min_size(egui::vec2(78.0, 34.0))
            .rounding(9.0),
    )
    .clicked()
}

fn status_color(status: &str) -> egui::Color32 {
    if status.contains("失败")
        || status.contains("错误")
        || status.contains("过期")
        || status.contains("没有")
        || status.contains("未登录")
    {
        egui::Color32::from_rgb(0xE0, 0x4F, 0x5F)
    } else if status.contains("已保存") || status.contains("成功") || status.contains("获取到")
    {
        egui::Color32::from_rgb(0x2E, 0xA0, 0x4E)
    } else {
        egui::Color32::from_gray(150)
    }
}

fn apply_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = egui::Color32::WHITE;
    style.visuals.window_fill = egui::Color32::WHITE;
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(25, 38, 60));
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(250, 251, 253);
    style.visuals.selection.bg_fill = egui::Color32::from_gray(225);
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
    style.visuals.hyperlink_color = ACCENT_DARK;
    for (text_style, size) in [
        (egui::TextStyle::Body, 14.0),
        (egui::TextStyle::Button, 13.0),
        (egui::TextStyle::Small, 12.0),
        (egui::TextStyle::Heading, 24.0),
    ] {
        style
            .text_styles
            .insert(text_style, egui::FontId::proportional(size));
    }
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(250);
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(225));
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(7.0);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(241);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(7.0);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(232);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.active.rounding = egui::Rounding::same(7.0);
    style.visuals.window_rounding = egui::Rounding::same(14.0);
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 32.0;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    ctx.set_style(style);
}

pub fn run() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_inner_size([1040.0, 700.0])
        .with_min_inner_size([780.0, 600.0])
        .with_icon(eframe::icon_data::from_png_bytes(LOGO_PNG).expect("Invalid application icon"))
        .with_title("拾文");
    eframe::run_native(
        "拾文",
        options,
        Box::new(|cc| {
            load_chinese_font(&cc.egui_ctx);
            apply_style(&cc.egui_ctx);
            Ok(Box::new(App::new(&cc.egui_ctx)))
        }),
    )
}

fn load_chinese_font(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "cjk".into());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}
