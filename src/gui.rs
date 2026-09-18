use crate::bili::{
    lines_to_srt, lines_to_txt, sanitize_filename, Client, QrPoll, SubTrack, VideoInfo, VideoStream,
};
use crate::cli::output_dir;
use crate::transcribe;
use anyhow::Result;
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Subtitle,
    Video,
    Transcribe,
}

enum Msg {
    VideoLoaded(Box<VideoInfo>, usize, Vec<SubTrack>, Vec<VideoStream>),
    TracksLoaded(usize, Vec<SubTrack>, Vec<VideoStream>),
    Saved(Result<String>),
    Failed(String),
    VideoStage(String),
    VideoProgress(f64),
    VideoSaved(Result<String>),
    TranscribeSaved(Result<Vec<PathBuf>>),
    QrReady {
        w: usize,
        pixels: Vec<egui::Color32>,
    },
    QrStage(String),
    QrConfirmed,
}

struct App {
    ffmpeg_ok: bool,
    mode: Mode,
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
    media_file: Option<PathBuf>,
    whisper_model: Option<PathBuf>,
    transcribe_language: String,
    output_dir: PathBuf,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    qr_texture: Option<egui::TextureHandle>,
    qr_stage: Option<String>,
    logged_in: bool,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();
        Self {
            mode: Mode::Subtitle,
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
            media_file: None,
            whisper_model: None,
            transcribe_language: "auto".into(),
            output_dir: output_dir(),
            tx,
            rx,
            qr_texture: None,
            qr_stage: None,
            logged_in: Client::has_saved_cookies(),
        }
    }

    fn spawn_fetch_video(&mut self) {
        let input = self.link.clone();
        if input.trim().is_empty() {
            self.status = "请先粘贴链接".into();
            return;
        }
        self.busy = true;
        self.tracks.clear();
        self.streams.clear();
        self.selected_track = 0;
        self.selected_stream = 0;
        self.status = "正在获取视频信息和画质列表...".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let client = Client::new();
            match client.fetch_video(&input) {
                Ok((video, page)) => {
                    let streams = client.fetch_streams(&video, page).unwrap_or_default();
                    let tracks = client.fetch_tracks(&video, page).unwrap_or_default();
                    let _ = tx.send(Msg::VideoLoaded(Box::new(video), page, tracks, streams));
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
        thread::spawn(move || {
            let client = Client::new();
            let streams = client.fetch_streams(&video, page).unwrap_or_default();
            let tracks = client.fetch_tracks(&video, page).unwrap_or_default();
            let _ = tx.send(Msg::TracksLoaded(page, tracks, streams));
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
                if stream.audio_url.is_some() && !Client::ffmpeg_available() {
                    anyhow::bail!(
                        "未检测到 ffmpeg，无法合并音视频。请先安装: winget install Gyan.FFmpeg"
                    );
                }

                let mut name = sanitize_filename(&title);
                if page > 1 {
                    name.push_str(&format!("_P{page}"));
                }
                name.push_str(&format!("_{}.mp4", stream.quality_id));
                std::fs::create_dir_all(&dir)?;
                let out_path = dir.join(&name);
                let video_tmp = dir.join(format!("{name}.video.tmp"));
                let audio_tmp = dir.join(format!("{name}.audio.tmp"));

                match &stream.audio_url {
                    Some(audio) => {
                        let progress_tx = tx.clone();
                        client.download_to_file(&stream.video_url, &video_tmp, &|p| {
                            let _ = progress_tx.send(Msg::VideoProgress(p));
                        })?;
                        let _ = tx.send(Msg::VideoStage("正在下载音频...".into()));
                        let progress_tx = tx.clone();
                        client.download_to_file(audio, &audio_tmp, &|p| {
                            let _ = progress_tx.send(Msg::VideoProgress(p));
                        })?;
                        let _ = tx.send(Msg::VideoStage("正在合并音视频...".into()));
                        let _ = tx.send(Msg::VideoProgress(0.0));
                        let status = std::process::Command::new("ffmpeg")
                            .args(["-y", "-i"])
                            .arg(&video_tmp)
                            .args(["-i"])
                            .arg(&audio_tmp)
                            .args(["-c", "copy"])
                            .arg(&out_path)
                            .output()?;
                        if !status.status.success() {
                            anyhow::bail!(
                                "ffmpeg 合并失败: {}",
                                String::from_utf8_lossy(&status.stderr)
                            );
                        }
                        let _ = std::fs::remove_file(&video_tmp);
                        let _ = std::fs::remove_file(&audio_tmp);
                    }
                    None => {
                        let progress_tx = tx.clone();
                        client.download_to_file(&stream.video_url, &out_path, &|p| {
                            let _ = progress_tx.send(Msg::VideoProgress(p));
                        })?;
                    }
                }
                Ok(out_path.display().to_string())
            })();
            let _ = tx.send(Msg::VideoSaved(res));
        });
    }

    fn spawn_transcribe(&mut self) {
        let Some(media) = self.media_file.clone() else {
            self.status = "请先选择音频或视频文件".into();
            return;
        };
        let Some(model) = self
            .whisper_model
            .clone()
            .or_else(|| transcribe::find_default_model())
        else {
            self.status = "未找到 Whisper 模型，请先选择模型文件".into();
            return;
        };
        let language = self.transcribe_language.clone();
        let dir = self.output_dir.clone();
        let tx = self.tx.clone();

        self.busy = true;
        self.status = "正在转换音频并识别文字...".into();
        thread::spawn(move || {
            let res = transcribe::run(&media, &model, &language, &dir);
            let _ = tx.send(Msg::TranscribeSaved(res));
        });
    }

    fn spawn_qr_login(&mut self) {
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
                match client.qr_poll(&qr.qrcode_key) {
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
            let _ = tx.send(Msg::Failed("登录超时，请重新扫码".into()));
        });
    }

    fn poll_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            self.busy = false;
            match msg {
                Msg::VideoLoaded(video, page, tracks, streams) => {
                    self.selected_page = page.min(video.pages.len().max(1));
                    self.selected_page = video
                        .pages
                        .iter()
                        .find(|p| p.page == page)
                        .map(|p| p.page)
                        .unwrap_or(1);
                    self.video = Some(*video);
                    self.tracks = tracks;
                    self.selected_track = 0;
                    self.streams = streams;
                    self.selected_stream = 0;
                    if self.streams.is_empty() {
                        self.status = "获取到视频信息，但没有可用画质".into();
                    } else if self.tracks.is_empty() {
                        if self.logged_in {
                            self.status = "没有可下载的字幕，但可以下载视频".into();
                        } else {
                            self.status = "未登录：AI字幕需要登录；当前仍可下载视频".into();
                        }
                    } else {
                        self.status = format!(
                            "获取到 {} 条字幕、{} 个画质",
                            self.tracks.len(),
                            self.streams.len()
                        );
                    }
                }
                Msg::TracksLoaded(page, tracks, streams) => {
                    self.selected_page = page;
                    self.tracks = tracks;
                    self.selected_track = 0;
                    self.streams = streams;
                    self.selected_stream = 0;
                    if self.streams.is_empty() {
                        self.status = "该分P没有可用画质".into();
                    } else if self.tracks.is_empty() {
                        self.status = "该分P没有可下载的字幕，但可以下载视频".into();
                    } else {
                        self.status = format!(
                            "获取到 {} 条字幕、{} 个画质",
                            self.tracks.len(),
                            self.streams.len()
                        );
                    }
                }
                Msg::VideoStage(s) => self.status = s,
                Msg::VideoProgress(p) => self.video_progress = Some(p.clamp(0.0, 1.0)),
                Msg::TranscribeSaved(res) => match res {
                    Ok(paths) => {
                        let texts: Vec<String> =
                            paths.iter().map(|p| p.display().to_string()).collect();
                        self.status = format!("已保存: {}", texts.join(" 和 "));
                    }
                    Err(e) => self.status = format!("识别失败: {e:#}"),
                },
                Msg::VideoSaved(res) => match res {
                    Ok(path) => {
                        self.video_progress = None;
                        self.status = format!("已保存: {path}");
                    }
                    Err(e) => {
                        self.video_progress = None;
                        self.status = format!("下载失败: {e:#}");
                    }
                },
                Msg::Saved(res) => match res {
                    Ok(path) => self.status = format!("已保存: {path}"),
                    Err(e) => self.status = format!("保存失败: {e:#}"),
                },
                Msg::Failed(e) => self.status = e,
                Msg::QrReady { w, pixels } => {
                    let image = egui::ColorImage {
                        size: [w, w],
                        pixels,
                    };
                    self.qr_texture =
                        Some(ctx.load_texture("qr", image, egui::TextureOptions::NEAREST));
                    self.qr_stage = Some("请用B站App扫一扫".into());
                    self.busy = false;
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
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_messages(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(16.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("B站字幕/视频下载器")
                            .size(22.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("Bilibili Subtitle & Video Downloader")
                            .size(11.0)
                            .color(egui::Color32::from_gray(150)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.logged_in {
                        if ui.small_button("退出登录").clicked() {
                            Client::clear_saved_cookies();
                            self.logged_in = false;
                            self.status = "已退出登录（AI字幕需要登录才能获取）".into();
                        }
                        egui::Frame::default()
                            .fill(egui::Color32::from_rgb(0xE6, 0xF4, 0xEA))
                            .rounding(10.0)
                            .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("已登录")
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(0x2E, 0xA0, 0x4E)),
                                );
                            });
                    } else if secondary_button(ui, "扫码登录", !self.busy) {
                        self.spawn_qr_login();
                    }
                });
            });

            ui.add_space(12.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("模式")
                        .size(13.0)
                        .color(egui::Color32::from_gray(120)),
                );
                if ui
                    .selectable_label(self.mode == Mode::Subtitle, "下载字幕")
                    .clicked()
                {
                    self.mode = Mode::Subtitle;
                }
                if ui
                    .selectable_label(self.mode == Mode::Video, "下载视频")
                    .clicked()
                {
                    self.mode = Mode::Video;
                }
                if ui
                    .selectable_label(self.mode == Mode::Transcribe, "音频/视频转文字")
                    .clicked()
                {
                    self.mode = Mode::Transcribe;
                }
            });
            ui.add_space(6.0);

            if self.mode != Mode::Transcribe {
                egui::Frame::default()
                    .fill(egui::Color32::WHITE)
                    .rounding(10.0)
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(228)))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let enabled = !self.busy;
                            ui.add_enabled(
                                enabled,
                                egui::TextEdit::singleline(&mut self.link)
                                    .hint_text(
                                        "https://www.bilibili.com/video/BV... 或 b23.tv 短链接",
                                    )
                                    .desired_width(ui.available_width() - 96.0),
                            );
                            let fetch_label = if self.mode == Mode::Video {
                                "获取视频"
                            } else {
                                "获取字幕"
                            };
                            if primary_button(ui, fetch_label, enabled) {
                                self.spawn_fetch_video();
                            }
                        });
                    });
            } else {
                egui::Frame::default()
                    .fill(egui::Color32::WHITE)
                    .rounding(10.0)
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(228)))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if secondary_button(ui, "选择音频/视频", !self.busy) {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter(
                                        "音频/视频",
                                        &[
                                            "mp4", "mkv", "flv", "mov", "avi", "webm", "mp3",
                                            "wav", "m4a", "aac", "flac", "ogg",
                                        ],
                                    )
                                    .pick_file()
                                {
                                    self.media_file = Some(path);
                                }
                            }
                            ui.label(
                                egui::RichText::new(
                                    self.media_file
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "尚未选择文件".into()),
                                )
                                .size(12.0)
                                .color(egui::Color32::from_gray(90)),
                            );
                        });

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("语言")
                                    .size(13.0)
                                    .color(egui::Color32::from_gray(120)),
                            );
                            egui::ComboBox::from_id_salt("transcribe_language")
                                .selected_text(match self.transcribe_language.as_str() {
                                    "zh" => "中文",
                                    "en" => "English",
                                    _ => "自动识别",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.transcribe_language,
                                        "auto".into(),
                                        "自动识别",
                                    );
                                    ui.selectable_value(
                                        &mut self.transcribe_language,
                                        "zh".into(),
                                        "中文",
                                    );
                                    ui.selectable_value(
                                        &mut self.transcribe_language,
                                        "en".into(),
                                        "English",
                                    );
                                });

                            if secondary_button(ui, "选择模型", !self.busy) {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Whisper 模型", &["bin", "ggml", "model"])
                                    .pick_file()
                                {
                                    self.whisper_model = Some(path);
                                }
                            }
                            let model = self
                                .whisper_model
                                .clone()
                                .or_else(|| transcribe::find_default_model());
                            ui.label(
                                egui::RichText::new(
                                    model
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "未找到模型".into()),
                                )
                                .size(12.0)
                                .color(egui::Color32::from_gray(90)),
                            );
                        });

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if primary_button(
                                ui,
                                "开始识别",
                                !self.busy && self.media_file.is_some() && self.ffmpeg_ok,
                            ) {
                                self.spawn_transcribe();
                            }
                            ui.label(
                                egui::RichText::new("识别完成后保存 TXT 和 SRT")
                                    .size(12.0)
                                    .color(egui::Color32::from_gray(150)),
                            );
                        });
                    });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(
                    egui::RichText::new("●")
                        .size(12.0)
                        .color(status_color(&self.status)),
                );
                ui.label(
                    egui::RichText::new(&self.status)
                        .size(13.0)
                        .color(egui::Color32::from_gray(70)),
                );
            });

            if let Some(stage) = &self.qr_stage {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if let Some(tex) = &self.qr_texture {
                        egui::Frame::default()
                            .fill(egui::Color32::WHITE)
                            .rounding(12.0)
                            .inner_margin(egui::Margin::same(16.0))
                            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(228)))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Image::new(tex)
                                        .fit_to_exact_size(egui::vec2(240.0, 240.0)),
                                );
                            });
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(stage)
                            .size(13.0)
                            .color(egui::Color32::from_gray(90)),
                    );
                });
            }

            let video_info = if self.mode == Mode::Transcribe {
                None
            } else {
                self.video.as_ref().map(|v| {
                    (
                        v.title.clone(),
                        format!("{} (aid {})", v.bvid, v.aid),
                        v.pages.clone(),
                    )
                })
            };
            if let Some((title, id_text, pages)) = &video_info {
                ui.add_space(10.0);
                egui::Frame::default()
                    .fill(egui::Color32::WHITE)
                    .rounding(10.0)
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(228)))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(title).size(15.0).strong());
                        ui.label(
                            egui::RichText::new(id_text)
                                .size(12.0)
                                .color(egui::Color32::from_gray(150)),
                        );

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("分P")
                                    .size(13.0)
                                    .color(egui::Color32::from_gray(120)),
                            );
                            let current = pages
                                .iter()
                                .find(|p| p.page == self.selected_page)
                                .map(|p| (p.page, p.part.clone()))
                                .unwrap_or((1, String::new()));
                            let prev_page = self.selected_page;
                            ui.add_enabled_ui(!self.busy && pages.len() > 1, |ui| {
                                egui::ComboBox::from_id_salt("page_select")
                                    .selected_text(format!("P{} {}", current.0, current.1))
                                    .show_ui(ui, |ui| {
                                        for p in pages {
                                            let label = format!("P{} {}", p.page, p.part);
                                            ui.selectable_value(
                                                &mut self.selected_page,
                                                p.page,
                                                label,
                                            );
                                        }
                                    });
                            });
                            if self.selected_page != prev_page {
                                self.spawn_fetch_tracks(self.selected_page);
                            }
                        });

                        ui.add_space(6.0);
                        if self.mode == Mode::Video {
                            let labels: Vec<String> = self
                                .streams
                                .iter()
                                .map(|s| {
                                    if s.quality_id == 0 {
                                        s.label.clone()
                                    } else {
                                        format!("{} [{}]", s.label, s.quality_id)
                                    }
                                })
                                .collect();
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("画质")
                                        .size(13.0)
                                        .color(egui::Color32::from_gray(120)),
                                );
                                ui.add_enabled_ui(!self.busy && !self.streams.is_empty(), |ui| {
                                    egui::ComboBox::from_id_salt("stream_select")
                                        .selected_text(
                                            labels
                                                .get(self.selected_stream)
                                                .cloned()
                                                .unwrap_or_else(|| "未获取到画质".into()),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (i, n) in labels.iter().enumerate() {
                                                ui.selectable_value(
                                                    &mut self.selected_stream,
                                                    i,
                                                    n,
                                                );
                                            }
                                        });
                                });
                            });

                            if self.streams.is_empty() && !self.busy {
                                ui.label(
                                    egui::RichText::new(
                                        "没有获取到可用画质，请换一个链接或分P试试",
                                    )
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(0xE0, 0x4F, 0x5F)),
                                );
                            }
                            if !self.ffmpeg_ok {
                                ui.label(
                                    egui::RichText::new(
                                        "未检测到 ffmpeg，B站音视频需要 ffmpeg 合并",
                                    )
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(0xC8, 0x74, 0x0F)),
                                );
                            }

                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if primary_button(
                                    ui,
                                    "下载视频",
                                    !self.busy && !self.streams.is_empty(),
                                ) {
                                    self.spawn_download_video();
                                }
                                ui.label(
                                    egui::RichText::new("输出为 MP4；高画质需要登录B站账号")
                                        .size(12.0)
                                        .color(egui::Color32::from_gray(150)),
                                );
                            });

                            if self.busy {
                                if let Some(progress) = self.video_progress {
                                    ui.add_space(6.0);
                                    ui.add(
                                        egui::ProgressBar::new(progress as f32)
                                            .desired_width(ui.available_width())
                                            .show_percentage(),
                                    );
                                }
                            }
                        } else if !self.tracks.is_empty() {
                            let names: Vec<String> = self
                                .tracks
                                .iter()
                                .map(|t| format!("{} [{}]", t.lan_doc, t.lan))
                                .collect();
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("语言")
                                        .size(13.0)
                                        .color(egui::Color32::from_gray(120)),
                                );
                                ui.add_enabled_ui(!self.busy, |ui| {
                                    egui::ComboBox::from_id_salt("track_select")
                                        .selected_text(
                                            names
                                                .get(self.selected_track)
                                                .cloned()
                                                .unwrap_or_default(),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (i, n) in names.iter().enumerate() {
                                                ui.selectable_value(&mut self.selected_track, i, n);
                                            }
                                        });
                                });
                            });

                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                let enabled = !self.busy;
                                if primary_button(ui, "下载 TXT", enabled) {
                                    self.spawn_download("txt");
                                }
                                if secondary_button(ui, "下载 SRT", enabled) {
                                    self.spawn_download("srt");
                                }
                            });
                        } else if !self.busy {
                            ui.label(
                                egui::RichText::new("没有找到可下载字幕；可以切换到「下载视频」")
                                    .size(12.0)
                                    .color(egui::Color32::from_gray(120)),
                            );
                        }
                    });
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("更改").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.output_dir = dir;
                        }
                    }
                    if ui.small_button("打开").clicked() {
                        let dir = self.output_dir.clone();
                        std::fs::create_dir_all(&dir).ok();
                        let _ = std::process::Command::new("explorer")
                            .arg(dir.as_os_str())
                            .spawn();
                    }
                    ui.label(
                        egui::RichText::new(format!("输出目录: {}", self.output_dir.display()))
                            .size(12.0)
                            .color(egui::Color32::from_gray(150)),
                    );
                });
            });
        });
    }
}

const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xFB, 0x72, 0x99);
const ACCENT_DARK: egui::Color32 = egui::Color32::from_rgb(0xE2, 0x5B, 0x83);

fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(label)
                .color(egui::Color32::WHITE)
                .strong(),
        )
        .fill(ACCENT)
        .rounding(8.0),
    )
    .clicked()
}

fn secondary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).color(ACCENT_DARK))
            .fill(ACCENT.gamma_multiply(0.12))
            .rounding(8.0),
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

fn apply_style(cc: &eframe::CreationContext<'_>) {
    let mut style = (*cc.egui_ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = egui::Color32::from_rgb(0xF6, 0xF7, 0xF9);
    style.visuals.window_fill = egui::Color32::from_rgb(0xF6, 0xF7, 0xF9);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
    style.visuals.hyperlink_color = ACCENT_DARK;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    cc.egui_ctx.set_style(style);
}

pub fn run() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_inner_size([760.0, 560.0])
        .with_min_inner_size([620.0, 460.0])
        .with_title("B站字幕下载器");
    eframe::run_native(
        "B站字幕下载器",
        options,
        Box::new(|cc| {
            load_chinese_font(cc);
            apply_style(cc);
            Ok(Box::new(App::new(cc)))
        }),
    )
}

fn load_chinese_font(cc: &eframe::CreationContext<'_>) {
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
            cc.egui_ctx.set_fonts(fonts);
            return;
        }
    }
}
