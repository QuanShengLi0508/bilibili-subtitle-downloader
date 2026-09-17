use crate::bili::{lines_to_srt, lines_to_txt, sanitize_filename, Client, QrPoll, SubTrack, VideoInfo};
use crate::cli::output_dir;
use anyhow::Result;
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

enum Msg {
    VideoLoaded(Box<VideoInfo>, usize, Vec<SubTrack>),
    TracksLoaded(usize, Vec<SubTrack>),
    Saved(Result<String>),
    Failed(String),
    QrReady { w: usize, pixels: Vec<egui::Color32> },
    QrStage(String),
    QrConfirmed,
}

struct App {
    link: String,
    status: String,
    busy: bool,
    video: Option<VideoInfo>,
    selected_page: usize,
    tracks: Vec<SubTrack>,
    selected_track: usize,
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
            link: String::new(),
            status: "粘贴B站视频链接，然后点「获取字幕」".into(),
            busy: false,
            video: None,
            selected_page: 1,
            tracks: Vec::new(),
            selected_track: 0,
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
        self.status = "正在获取视频信息...".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let client = Client::new();
            match client.fetch_video(&input) {
                Ok((video, page)) => match client.fetch_tracks(&video, page) {
                    Ok(tracks) => {
                        let _ = tx.send(Msg::VideoLoaded(Box::new(video), page, tracks));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::Failed(format!("获取字幕列表失败: {e:#}")));
                    }
                },
                Err(e) => {
                    let _ = tx.send(Msg::Failed(format!("获取视频信息失败: {e:#}")));
                }
            }
        });
    }

    fn spawn_fetch_tracks(&mut self, page: usize) {
        let Some(video) = self.video.as_ref() else { return };
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
        self.status = format!("正在获取第 {page} 个分P的字幕...");
        self.tracks.clear();
        self.selected_track = 0;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let client = Client::new();
            match client.fetch_tracks(&video, page) {
                Ok(tracks) => {
                    let _ = tx.send(Msg::TracksLoaded(page, tracks));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(format!("获取字幕列表失败: {e:#}")));
                }
            }
        });
    }

    fn spawn_download(&mut self, fmt: &str) {
        let Some(video) = self.video.as_ref() else { return };
        let Some(track) = self.tracks.get(self.selected_track) else { return };
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
                Msg::VideoLoaded(video, page, tracks) => {
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
                    if self.tracks.is_empty() {
                        if self.logged_in {
                            self.status = "该视频没有可下载的字幕（可能未上传CC字幕）".into();
                        } else {
                            self.status = "未登录：AI字幕需要登录。请先点「扫码登录」，登录成功后会自动重新获取".into();
                        }
                    } else {
                        self.status = format!("获取到 {} 条字幕，选择语言后即可下载", self.tracks.len());
                    }
                }
                Msg::TracksLoaded(page, tracks) => {
                    self.selected_page = page;
                    self.tracks = tracks;
                    self.selected_track = 0;
                    if self.tracks.is_empty() {
                        self.status = "该分P没有可下载的字幕".into();
                    } else {
                        self.status = format!("获取到 {} 条字幕", self.tracks.len());
                    }
                }
                Msg::Saved(res) => match res {
                    Ok(path) => self.status = format!("已保存: {path}"),
                    Err(e) => self.status = format!("保存失败: {e:#}"),
                },
                Msg::Failed(e) => self.status = e,
                Msg::QrReady { w, pixels } => {
                    let image = egui::ColorImage { size: [w, w], pixels };
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
            ui.add_space(8.0);
            ui.heading("B站字幕下载器");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let enabled = !self.busy;
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut self.link)
                        .hint_text("https://www.bilibili.com/video/BV... 或 b23.tv 短链接")
                        .desired_width(ui.available_width() - 110.0),
                );
                ui.add_enabled(enabled, egui::Button::new("获取字幕"))
                    .clicked()
                    .then(|| self.spawn_fetch_video());
                ui.add_enabled(enabled, egui::Button::new("扫码登录"))
                    .clicked()
                    .then(|| self.spawn_qr_login());
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if self.logged_in {
                    ui.label(egui::RichText::new("已登录").strong());
                    if ui.small_button("退出登录").clicked() {
                        Client::clear_saved_cookies();
                        self.logged_in = false;
                        self.status = "已退出登录（AI字幕需要登录才能获取）".into();
                    }
                } else {
                    ui.label("未登录：AI字幕需要登录后才能获取");
                }
            });
            if let Some(stage) = &self.qr_stage {
                ui.vertical_centered(|ui| {
                    if let Some(tex) = &self.qr_texture {
                        ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(260.0, 260.0)));
                    }
                    ui.label(stage);
                });
            }

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(&self.status)
                    .weak()
                    .size(12.0),
            );
            ui.separator();

            let video_info = self.video.as_ref().map(|v| {
                (
                    v.title.clone(),
                    format!("{} (aid {})", v.bvid, v.aid),
                    v.pages.clone(),
                )
            });
            if let Some((title, id_text, pages)) = &video_info {
                ui.label(format!("标题: {title}"));
                ui.label(format!("ID: {id_text}"));

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("分P:");
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
                                    ui.selectable_value(&mut self.selected_page, p.page, label);
                                }
                            });
                    });
                    if self.selected_page != prev_page {
                        self.spawn_fetch_tracks(self.selected_page);
                    }
                });

                ui.add_space(4.0);
                if !self.tracks.is_empty() {
                    let names: Vec<String> = self
                        .tracks
                        .iter()
                        .map(|t| format!("{} [{}]", t.lan_doc, t.lan))
                        .collect();
                    ui.label("字幕语言:");
                    ui.add_enabled_ui(!self.busy, |ui| {
                        egui::ComboBox::from_id_salt("track_select")
                            .selected_text(names.get(self.selected_track).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.selected_track, i, n);
                                }
                            });
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let enabled = !self.busy;
                        if ui
                            .add_enabled(enabled, egui::Button::new("下载 TXT"))
                            .clicked()
                        {
                            self.spawn_download("txt");
                        }
                        if ui
                            .add_enabled(enabled, egui::Button::new("下载 SRT"))
                            .clicked()
                        {
                            self.spawn_download("srt");
                        }
                    });
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(format!("输出目录: {}", self.output_dir.display()));
                if ui.button("浏览...").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.output_dir = dir;
                    }
                }
                if ui.button("打开输出文件夹").clicked() {
                    let dir = self.output_dir.clone();
                    std::fs::create_dir_all(&dir).ok();
                    let _ = std::process::Command::new("explorer")
                        .arg(dir.as_os_str())
                        .spawn();
                }
            });
        });
    }
}

pub fn run() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_inner_size([680.0, 480.0])
        .with_min_inner_size([560.0, 380.0])
        .with_title("B站字幕下载器");
    eframe::run_native(
        "B站字幕下载器",
        options,
        Box::new(|cc| {
            load_chinese_font(cc);
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
