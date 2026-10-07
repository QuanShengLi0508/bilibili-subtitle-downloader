use super::*;

const INK: egui::Color32 = egui::Color32::from_rgb(25, 38, 60);
const MUTED: egui::Color32 = egui::Color32::from_rgb(101, 116, 139);
const BLUE: egui::Color32 = egui::Color32::from_rgb(35, 35, 35);

#[derive(Clone, Copy)]
pub(super) enum Symbol {
    Document,
    Play,
    Wave,
    Folder,
    Info,
}

pub(super) fn icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    symbol: Symbol,
    color: egui::Color32,
) {
    let point = |x: f32, y: f32| {
        egui::pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    };
    let stroke = egui::Stroke::new(2.0_f32, color);
    match symbol {
        Symbol::Wave => {
            for (x, height) in [
                (0.18, 0.28),
                (0.34, 0.62),
                (0.5, 0.86),
                (0.66, 0.62),
                (0.82, 0.28),
            ] {
                painter.line_segment(
                    [point(x, 0.5 - height / 2.0), point(x, 0.5 + height / 2.0)],
                    egui::Stroke::new((rect.width() * 0.09).max(2.0), color),
                );
            }
        }
        Symbol::Play => {
            painter.rect_stroke(
                egui::Rect::from_min_max(point(0.07, 0.17), point(0.93, 0.83)),
                3.0,
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![point(0.42, 0.32), point(0.42, 0.68), point(0.7, 0.5)],
                color,
                egui::Stroke::NONE,
            ));
        }
        Symbol::Document => {
            painter.add(egui::Shape::closed_line(
                vec![
                    point(0.22, 0.06),
                    point(0.61, 0.06),
                    point(0.8, 0.25),
                    point(0.8, 0.94),
                    point(0.22, 0.94),
                ],
                stroke,
            ));
            for y in [0.42, 0.58, 0.74] {
                painter.line_segment([point(0.34, y), point(0.67, y)], stroke);
            }
            painter.line_segment([point(0.6, 0.07), point(0.6, 0.26)], stroke);
            painter.line_segment([point(0.6, 0.26), point(0.78, 0.26)], stroke);
        }
        Symbol::Folder => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    point(0.08, 0.23),
                    point(0.38, 0.23),
                    point(0.49, 0.37),
                    point(0.91, 0.37),
                    point(0.91, 0.84),
                    point(0.08, 0.84),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        Symbol::Info => {
            painter.circle_filled(rect.center(), rect.width() * 0.46, color);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "i",
                egui::FontId::proportional(rect.height() * 0.72),
                egui::Color32::WHITE,
            );
        }
    }
}

pub(super) fn wide_button(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    enabled: bool,
    symbol: Symbol,
) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 42.0), egui::Sense::click());
        ui.painter().rect_filled(
            rect,
            12.0,
            if !ui.is_enabled() {
                egui::Color32::from_gray(207)
            } else if response.hovered() {
                egui::Color32::from_gray(62)
            } else {
                BLUE
            },
        );
        let galley = ui.painter().layout_no_wrap(
            label.into(),
            egui::FontId::proportional(17.0),
            egui::Color32::WHITE,
        );
        let x = rect.center().x - (galley.size().x + 34.0) / 2.0;
        icon(
            ui.painter(),
            egui::Rect::from_center_size(
                egui::pos2(x + 10.0, rect.center().y),
                egui::vec2(22.0, 22.0),
            ),
            symbol,
            egui::Color32::WHITE,
        );
        ui.painter().galley(
            egui::pos2(x + 34.0, rect.center().y - galley.size().y / 2.0),
            galley,
            egui::Color32::WHITE,
        );
        response.clicked()
            || (response.has_focus()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
                }))
    })
    .inner
}

impl App {
    pub(super) fn render_reference(&mut self, ctx: &egui::Context) {
        self.poll_messages(ctx);
        let compact = ctx.screen_rect().height() < 680.0;
        let margin = if compact { 16.0 } else { 24.0 };
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.mode == Mode::Transcribe && !self.busy {
            let dropped = ctx.input(|input| input.raw.dropped_files.clone());
            if !dropped.is_empty() {
                if dropped.len() != 1 {
                    self.status = "一次请选择一个音频或视频文件".into();
                } else if let Some(path) = &dropped[0].path {
                    if supported_media(path) {
                        self.media_file = Some(path.clone());
                        self.status = "文件已就绪，选择格式后点击开始识别".into();
                    } else {
                        self.status = "暂不支持此文件，请选择音频或视频".into();
                    }
                }
            }
        }
        egui::SidePanel::left("platform_sidebar")
            .exact_width(if ctx.screen_rect().width() < 900.0 {
                142.0
            } else {
                178.0
            })
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(249, 249, 249))
                    .inner_margin(egui::Margin::symmetric(12.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.image((self.logo.id(), egui::vec2(28.0, 28.0)));
                    ui.label(egui::RichText::new("拾文").size(20.0).strong().color(INK));
                });
                ui.add_space(if compact { 14.0 } else { 23.0 });
                ui.label(egui::RichText::new("内容平台").size(11.0).color(MUTED));
                ui.add_space(7.0);
                for (mode, label, symbol) in [
                    (Mode::Subtitle, "哔哩哔哩", Symbol::Play),
                    (Mode::Douyin, "抖音", Symbol::Play),
                    (Mode::Xhs, "小红书", Symbol::Document),
                    (Mode::Youtube, "YouTube", Symbol::Play),
                    (Mode::WebText, "知乎", Symbol::Document),
                    (Mode::WechatChannels, "微信视频号", Symbol::Play),
                    (Mode::WechatArticle, "微信公众号", Symbol::Document),
                    (Mode::Transcribe, "本地转写", Symbol::Wave),
                ] {
                    let selected = self.mode == mode
                        || (mode == Mode::Subtitle
                            && matches!(self.mode, Mode::Video | Mode::BiliText));
                    ui.add_enabled_ui(!self.busy, |ui| {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), if compact { 33.0 } else { 37.0 }),
                            egui::Sense::click(),
                        );
                        if selected || response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                8.0,
                                egui::Color32::from_gray(if selected { 233 } else { 241 }),
                            );
                        }
                        let color = if selected { INK } else { MUTED };
                        icon(
                            ui.painter(),
                            egui::Rect::from_center_size(
                                egui::pos2(rect.left() + 17.0, rect.center().y),
                                egui::vec2(17.0, 17.0),
                            ),
                            symbol,
                            color,
                        );
                        ui.painter().text(
                            egui::pos2(rect.left() + 35.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(13.0),
                            color,
                        );
                        if response.clicked()
                            || (response.has_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                        {
                            if !selected {
                                self.mode = mode;
                                self.clear_wechat();
                                self.comments = None;
                                self.comment_selected.clear();
                                self.comment_window_open = false;
                                self.status = if mode == Mode::Transcribe {
                                    "选择音频或视频文件，然后开始识别"
                                } else {
                                    "粘贴链接，选择内容类型后获取"
                                }
                                .into();
                                self.youtube = None;
                                self.external = None;
                                self.video = None;
                                self.douyin_article = None;
                                self.zhihu_content = None;
                            }
                        }
                    });
                }
                if !self.saved_files.is_empty() {
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new("最近文件").size(11.0).color(MUTED));
                    ui.add_space(6.0);
                    for path in
                        self.saved_files
                            .clone()
                            .into_iter()
                            .take(if compact { 2 } else { 3 })
                    {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 28.0).max(40.0);
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            let response = ui
                                .add_sized(
                                    [width, 28.0],
                                    egui::Label::new(
                                        egui::RichText::new(name).size(12.0).color(INK),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_text(format!("点击打开\n{}", path.display()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if response.clicked() {
                                if let Err(error) = open_saved_file(&path, false) {
                                    self.status = format!("打开失败：{error}");
                                }
                            }
                            ui.menu_button("...", |ui| {
                                if ui.button("打开文件").clicked() {
                                    if let Err(error) = open_saved_file(&path, false) {
                                        self.status = format!("打开失败：{error}");
                                    }
                                    ui.close_menu();
                                }
                                if ui.button("所在位置").clicked() {
                                    if let Err(error) = open_saved_file(&path, true) {
                                        self.status = format!("打开失败：{error}");
                                    }
                                    ui.close_menu();
                                }
                            });
                        });
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui
                        .add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new("输出目录").frame(false),
                        )
                        .on_hover_text(self.output_dir.display().to_string())
                        .clicked()
                    {
                        self.output_settings_open = true;
                    }
                });
            });
        egui::TopBottomPanel::top("reference_header")
            .frame(
                egui::Frame::default()
                    .fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        egui::Color32::from_rgb(232, 236, 243),
                    ))
                    .inner_margin(egui::Margin::symmetric(
                        margin,
                        if compact { 8.0 } else { 12.0 },
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let title = match self.mode {
                            Mode::Subtitle | Mode::Video | Mode::BiliText => "哔哩哔哩",
                            Mode::Douyin => "抖音",
                            Mode::Xhs => "小红书",
                            Mode::Youtube => "YouTube",
                            Mode::WebText => "知乎",
                            Mode::Transcribe => "本地转写",
                            Mode::WechatArticle => "微信公众号",
                            Mode::WechatChannels => "微信视频号",
                        };
                        ui.label(egui::RichText::new(title).size(17.0).strong().color(INK));
                        ui.label(
                            egui::RichText::new(if self.mode == Mode::Transcribe {
                                "音频与视频，整理成文字"
                            } else {
                                "保存视频、图文和文字"
                            })
                            .size(11.0)
                            .color(MUTED),
                        );
                    });
                    ui.add_space((ui.available_width() - 150.0).max(0.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            if self.is_wechat() {
                                if secondary_button(
                                    ui,
                                    "打开官方页面",
                                    !self.busy && !self.link.trim().is_empty(),
                                ) {
                                    self.spawn_wechat_reader();
                                }
                                ui.label(
                                    egui::RichText::new("在网页内完成登录或验证")
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            } else if self.mode == Mode::Youtube {
                                if secondary_button(ui, "导入登录状态", !self.busy) {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("登录状态", &["txt"])
                                        .pick_file()
                                    {
                                        self.status = match crate::youtube::import_cookies(&path) {
                                            Ok(()) => "YouTube 登录状态已导入，请重新获取".into(),
                                            Err(error) => format!("{error:#}"),
                                        };
                                    }
                                }
                                ui.label(
                                    egui::RichText::new("需能访问 YouTube")
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            } else if self.mode == Mode::Douyin {
                                if secondary_button(ui, "抖音登录", !self.busy) {
                                    self.spawn_douyin_login();
                                }
                                ui.label(
                                    egui::RichText::new(
                                        if external::douyin_cookie_path().is_file() {
                                            "已保存访问会话"
                                        } else {
                                            "无法解析时可尝试登录"
                                        },
                                    )
                                    .size(11.0)
                                    .color(MUTED),
                                );
                            } else if self.mode == Mode::Xhs {
                                if secondary_button(ui, "小红书登录", !self.busy) {
                                    self.spawn_xhs_login();
                                }
                                ui.label(
                                    egui::RichText::new("需要时在官方网页登录")
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            } else if self.mode == Mode::WebText {
                                if secondary_button(ui, "知乎登录", !self.busy) {
                                    self.spawn_zhihu_login();
                                }
                                ui.label(
                                    egui::RichText::new(
                                        if crate::zhihu_login::has_saved_session() {
                                            "知乎已登录"
                                        } else {
                                            "登录后获取完整正文"
                                        },
                                    )
                                    .size(11.0)
                                    .color(MUTED),
                                );
                            } else if self.mode == Mode::Transcribe {
                                ui.label(egui::RichText::new("本地处理").size(12.0).color(BLUE));
                                ui.label(
                                    egui::RichText::new("无需登录，选择文件即可")
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            } else if self.logged_in {
                                if secondary_button(ui, "退出登录", !self.busy) {
                                    Client::clear_saved_cookies();
                                    self.logged_in = false;
                                    self.status = "已退出登录".into();
                                }
                                ui.label(egui::RichText::new("B站已登录").size(12.0).color(MUTED));
                            } else {
                                if secondary_button(ui, "扫码登录", !self.busy) {
                                    self.spawn_qr_login();
                                }
                                ui.label(
                                    egui::RichText::new("登录后获取 AI 字幕")
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            }
                        });
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(
                        margin,
                        if compact { 10.0 } else { 16.0 },
                    )),
            )
            .show(ctx, |ui| {
                let bottom = ui.max_rect().bottom();
                ui.horizontal(|ui| {
                    let width = ui.available_width().min(780.0);
                    ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_width(width);
                            egui::Frame::default()
                                .fill(egui::Color32::WHITE)
                                .inner_margin(egui::Margin::same(if compact { 4.0 } else { 10.0 }))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    self.source_card(ui);
                                    self.media_card(ui);
                                    self.result_card(ui);
                                });
                        },
                    );
                });
                #[cfg(test)]
                if ui.min_rect().bottom() > bottom + 1.0 {
                    eprintln!(
                        "overflow mode {}: used bottom {}, available bottom {}",
                        self.mode as u8,
                        ui.min_rect().bottom(),
                        bottom
                    );
                }
                #[cfg(test)]
                ctx.data_mut(|data| {
                    data.insert_temp(
                        egui::Id::new("workspace_fits"),
                        ui.min_rect().bottom() <= bottom + 1.0,
                    )
                });
                let _ = bottom;
            });
        self.render_comments(ctx);
        self.wechat_preview(ctx);
        if self.output_settings_open {
            let mut open = self.output_settings_open;
            egui::Window::new("输出目录")
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(460.0)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("文件保存位置").strong());
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(self.output_dir.display().to_string()).truncate())
                        .on_hover_text(self.output_dir.display().to_string());
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if secondary_button(ui, "更改位置", !self.busy) {
                            if let Some(dir) = rfd::FileDialog::new()
                                .set_directory(&self.output_dir)
                                .pick_folder()
                            {
                                self.output_dir = dir;
                            }
                        }
                        if secondary_button(ui, "打开文件夹", true) {
                            let result = std::fs::create_dir_all(&self.output_dir).and_then(|_| {
                                std::process::Command::new("explorer.exe")
                                    .arg(&self.output_dir)
                                    .spawn()
                            });
                            if let Err(error) = result {
                                self.status = format!("打开目录失败：{error}");
                            }
                        }
                    });
                });
            self.output_settings_open = open;
        }
        self.login_window(ctx);
        if let Some(article) = self.douyin_article.clone() {
            egui::Window::new("图文 / 文章 · 全部文字预览")
                .open(&mut self.douyin_preview_open)
                .default_width(640.0)
                .show(ctx, |ui| {
                    ui.label(&article.title);
                    egui::ScrollArea::vertical()
                        .max_height((ctx.screen_rect().height() - 180.0).max(150.0))
                        .show(ui, |ui| {
                            ui.label(&article.body);
                        });
                });
        }
    }

    pub(super) fn file_drop_zone(&mut self, ui: &mut egui::Ui) {
        let hovered = ui.input(|input| !input.raw.hovered_files.is_empty()) && !self.busy;
        egui::Frame::default()
            .fill(if hovered {
                egui::Color32::from_rgb(227, 240, 255)
            } else {
                egui::Color32::from_gray(250)
            })
            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(222)))
            .rounding(13.0)
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let rect = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(ui.available_width(), 70.0),
                );
                dashed_border(ui.painter(), rect, egui::Color32::from_gray(218));
                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(38.0, 70.0), egui::Sense::hover());
                    icon(
                        ui.painter(),
                        egui::Rect::from_center_size(icon_rect.center(), egui::vec2(30.0, 30.0)),
                        Symbol::Folder,
                        BLUE,
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2((ui.available_width() - 190.0).max(150.0), 70.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.add_space(10.0);
                            let label = if hovered {
                                "松开鼠标，添加文件".into()
                            } else {
                                self.media_file
                                    .as_ref()
                                    .and_then(|path| path.file_name())
                                    .map(|name| name.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "选择音频/视频文件".into())
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(label).size(16.0).strong().color(INK),
                                )
                                .truncate(),
                            );
                            ui.label(
                                egui::RichText::new("拖入文件，或点击右侧选择文件")
                                    .size(12.0)
                                    .color(MUTED),
                            );
                            ui.label(
                                egui::RichText::new("支持 MP4、MKV、MP3、WAV 等格式")
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        },
                    );
                    if wide_button(
                        ui,
                        if self.media_file.is_some() {
                            "更换文件"
                        } else {
                            "选择文件"
                        },
                        176.0,
                        !self.busy,
                        Symbol::Folder,
                    ) {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                "音频 / 视频",
                                &[
                                    "mp4", "mkv", "flv", "mov", "avi", "wmv", "webm", "mp3", "wav",
                                    "m4a", "aac", "flac", "ogg",
                                ],
                            )
                            .pick_file()
                        {
                            self.media_file = Some(path);
                        }
                    }
                });
            });
    }
}

fn dashed_border(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    for (a, b) in [
        (
            rect.left_top() + egui::vec2(8.0, 0.0),
            rect.right_top() - egui::vec2(8.0, 0.0),
        ),
        (
            rect.left_bottom() + egui::vec2(8.0, 0.0),
            rect.right_bottom() - egui::vec2(8.0, 0.0),
        ),
        (
            rect.left_top() + egui::vec2(0.0, 8.0),
            rect.left_bottom() - egui::vec2(0.0, 8.0),
        ),
        (
            rect.right_top() + egui::vec2(0.0, 8.0),
            rect.right_bottom() - egui::vec2(0.0, 8.0),
        ),
    ] {
        let distance = a.distance(b);
        let direction = (b - a).normalized();
        let mut offset = 0.0;
        while offset < distance {
            painter.line_segment(
                [
                    a + direction * offset,
                    a + direction * (offset + 5.0).min(distance),
                ],
                egui::Stroke::new(1.0_f32, color),
            );
            offset += 9.0;
        }
    }
}

fn supported_media(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp4"
                    | "mkv"
                    | "flv"
                    | "mov"
                    | "avi"
                    | "wmv"
                    | "webm"
                    | "mp3"
                    | "wav"
                    | "m4a"
                    | "aac"
                    | "flac"
                    | "ogg"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn douyin_tab_rejects_other_sites_and_explains_subtitle_mismatch() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.mode = Mode::Douyin;
        app.link = "https://www.bilibili.com/video/BV1example".into();
        app.spawn_fetch_video();
        assert!(!app.busy);
        assert!(app.status.contains("请粘贴抖音"));
        app.mode = Mode::Subtitle;
        app.link = "https://www.douyin.com/jingxuan?modal_id=7673787196373060900".into();
        app.spawn_fetch_video();
        assert!(!app.busy);
        assert!(app.status.contains("抖音"));
        app.mode = Mode::Xhs;
        app.spawn_fetch_video();
        assert!(!app.busy);
        assert!(app.status.contains("请粘贴小红书"));
    }

    #[test]
    fn dropped_media_is_selected_and_busy_operations_are_preserved() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.mode = Mode::Transcribe;
        let input = |path: &str| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1040.0, 840.0),
            )),
            dropped_files: vec![egui::DroppedFile {
                path: Some(PathBuf::from(path)),
                ..Default::default()
            }],
            ..Default::default()
        };
        let _ = ctx.run(input("recording.MP4"), |ctx| app.render_reference(ctx));
        assert_eq!(app.media_file, Some(PathBuf::from("recording.MP4")));
        app.busy = true;
        let _ = ctx.run(input("different.wav"), |ctx| app.render_reference(ctx));
        assert_eq!(app.media_file, Some(PathBuf::from("recording.MP4")));
        app.busy = false;
        let _ = ctx.run(input("document.pdf"), |ctx| app.render_reference(ctx));
        assert_eq!(app.media_file, Some(PathBuf::from("recording.MP4")));
        assert!(app.status.contains("不支持"));
    }
}
