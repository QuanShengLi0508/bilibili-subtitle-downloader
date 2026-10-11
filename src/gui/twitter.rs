use super::*;

const X_INK: egui::Color32 = egui::Color32::from_rgb(23, 36, 59);
const X_MUTED: egui::Color32 = egui::Color32::from_rgb(108, 124, 149);
const X_BLUE: egui::Color32 = egui::Color32::from_rgb(56, 104, 232);
const X_BORDER: egui::Color32 = egui::Color32::from_rgb(222, 230, 242);

fn x_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(12.0).color(if selected {
            X_BLUE
        } else {
            X_MUTED
        }))
        .fill(if selected {
            egui::Color32::from_rgb(236, 242, 255)
        } else {
            egui::Color32::WHITE
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if selected { X_BLUE } else { X_BORDER },
        ))
        .rounding(8.0)
        .min_size(egui::vec2(0.0, 27.0)),
    )
}

impl App {
    fn spawn_twitter(&mut self, browser: bool) {
        let Some(input) = crate::twitter::link(&self.link) else {
            self.status = "请粘贴 X / Twitter 的单条帖子分享链接".into();
            return;
        };
        let tool = external::find_yt_dlp();
        if !browser && tool.is_none() {
            self.status = "下载组件未就绪，请使用官方网页读取文字或重新安装应用".into();
            return;
        }
        self.clear_content();
        self.busy = true;
        self.status = if browser {
            "在 X 官方页面完成登录、展开目标帖子，然后点击读取当前帖子"
        } else {
            "正在获取 X 帖子的文字、图片和视频信息…"
        }
        .into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = if browser {
                crate::twitter::read(&input)
            } else {
                crate::twitter::probe(&input, &tool.unwrap())
            };
            let _ = tx.send(Msg::TwitterLoaded(result));
        });
    }

    fn save_twitter(&mut self) {
        let Some(post) = self.twitter_post.clone() else {
            return;
        };
        let directory = self.output_dir.clone();
        let format = self.text_format;
        let video = self.twitter_video_mode;
        let tool = external::find_yt_dlp();
        if video && tool.is_none() {
            self.status = "视频下载组件未就绪，请重新安装应用".into();
            return;
        }
        self.busy = true;
        self.status = if video {
            "正在下载 X 视频…"
        } else {
            "正在保存 X 帖子文字与配图…"
        }
        .into();
        self.video_progress = video.then_some(0.0);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = if video {
                crate::twitter::download(&post, &tool.unwrap(), &directory, &|progress| {
                    let _ = tx.send(Msg::VideoProgress(progress));
                })
            } else {
                post.export(&directory, format).map(|path| vec![path])
            };
            let _ = tx.send(Msg::TwitterSaved(result));
        });
    }

    pub(super) fn twitter_source_card(&mut self, ui: &mut egui::Ui) {
        let compact = ui.ctx().screen_rect().height() < 680.0;
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("内容类型").size(12.0).color(X_MUTED));
                if x_chip(ui, "图文 / 文字", !self.twitter_video_mode).clicked() {
                    self.twitter_video_mode = false;
                }
                if x_chip(ui, "视频", self.twitter_video_mode).clicked() {
                    self.twitter_video_mode = true;
                }
                ui.separator();
                if secondary_button(ui, "检测评论", true) {
                    self.spawn_comments();
                }
                if self.comments.is_some() && secondary_button(ui, "选择评论", true) {
                    self.comment_window_open = true;
                }
            });
        });
        ui.add_space(if compact { 6.0 } else { 10.0 });
        ui.label(
            egui::RichText::new("帖子链接")
                .size(13.0)
                .strong()
                .color(X_INK),
        );
        let response = egui::Frame::default()
            .fill(egui::Color32::from_rgb(249, 251, 255))
            .stroke(egui::Stroke::new(1.0_f32, X_BORDER))
            .rounding(10.0)
            .inner_margin(egui::Margin::symmetric(12.0, 7.0))
            .show(ui, |ui| {
                ui.add_enabled(
                    !self.busy,
                    egui::TextEdit::singleline(&mut self.link)
                        .frame(false)
                        .desired_width(ui.available_width())
                        .margin(egui::vec2(2.0, 3.0))
                        .hint_text("https://x.com/作者/status/… 或 twitter.com 分享链接"),
                )
            })
            .inner;
        if response.changed() && self.link_changed() {
            return;
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.busy {
            self.spawn_twitter(false);
        }
        ui.add_space(if compact { 6.0 } else { 10.0 });
        if !self.twitter_video_mode {
            ui.add_enabled_ui(!self.busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("导出格式").size(12.0).color(X_MUTED));
                    for format in TextFormat::ALL {
                        if x_chip(ui, format.label(), self.text_format == format).clicked() {
                            self.text_format = format;
                        }
                    }
                });
            });
        }
        if let Some(post) = self.twitter_post.clone() {
            ui.add_space(if compact { 6.0 } else { 10.0 });
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(247, 250, 255))
                .stroke(egui::Stroke::new(1.0_f32, X_BORDER))
                .rounding(11.0)
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("已获取")
                                .size(11.0)
                                .strong()
                                .color(X_BLUE),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&post.title)
                                    .size(13.0)
                                    .strong()
                                    .color(X_INK),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&post.title);
                    });
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {} 张配图 · {} 个视频",
                            post.author,
                            post.images.len(),
                            post.video_count
                        ))
                        .size(11.0)
                        .color(X_MUTED),
                    );
                    if !compact {
                        let excerpt = post
                            .body
                            .chars()
                            .take(120)
                            .collect::<String>()
                            .replace('\n', " ");
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(excerpt).size(11.0).color(X_MUTED),
                            )
                            .truncate(),
                        );
                    }
                    if let Some(warning) = &post.warning {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(warning)
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(150, 95, 30)),
                            )
                            .truncate(),
                        )
                        .on_hover_text(warning);
                    }
                    ui.horizontal(|ui| {
                        if secondary_button(ui, "查看全文", true) {
                            self.twitter_preview_open = true;
                        }
                        if secondary_button(ui, "重新获取", !self.busy) {
                            self.spawn_twitter(false);
                        }
                    });
                    if self.twitter_video_mode && post.video_count == 0 {
                        ui.label(
                            egui::RichText::new(
                                "未检测到视频，可切换到图文 / 文字，或在官方网页确认。",
                            )
                            .size(11.0)
                            .color(X_MUTED),
                        );
                    }
                });
        }
        ui.add_space(if compact { 8.0 } else { 12.0 });
        let ready = self.twitter_post.is_some();
        let enabled = !self.busy
            && !self.link.trim().is_empty()
            && (!ready
                || !self.twitter_video_mode
                || self
                    .twitter_post
                    .as_ref()
                    .is_some_and(|p| p.video_count > 0));
        let label = if self.busy {
            "正在处理…"
        } else if !ready {
            "获取 X 帖子"
        } else if self.twitter_video_mode {
            "下载帖子视频"
        } else {
            "确认导出文字与配图"
        };
        if super::reference::wide_button(
            ui,
            label,
            ui.available_width(),
            enabled,
            super::reference::Symbol::Play,
        ) {
            if ready {
                self.save_twitter();
            } else {
                self.spawn_twitter(false);
            }
        }
        ui.add_space(if compact { 4.0 } else { 7.0 });
        ui.horizontal_wrapped(|ui| {
            if secondary_button(
                ui,
                "官方网页读取",
                !self.busy && !self.link.trim().is_empty(),
            ) {
                self.spawn_twitter(true);
            }
            ui.label(
                egui::RichText::new("长文不完整或需要登录时，在网页内展开后读取")
                    .size(11.0)
                    .color(X_MUTED),
            );
        });
        ui.label(
            egui::RichText::new("仅获取当前帖子；评论需单独检测并勾选，图片文字不做 OCR。")
                .size(11.0)
                .color(X_MUTED),
        );
    }

    pub(super) fn twitter_preview(&mut self, ctx: &egui::Context) {
        if let Some(post) = self.twitter_post.clone() {
            egui::Window::new("X / 推特 · 全文预览")
                .open(&mut self.twitter_preview_open)
                .default_width(640.0)
                .max_width((ctx.screen_rect().width() - 80.0).max(300.0))
                .show(ctx, |ui| {
                    ui.heading(&post.title);
                    if let Some(warning) = &post.warning {
                        ui.label(warning);
                    }
                    egui::ScrollArea::vertical()
                        .max_height((ctx.screen_rect().height() - 190.0).max(150.0))
                        .show(ui, |ui| {
                            ui.label(&post.body);
                        });
                });
        }
    }
}
