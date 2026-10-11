use super::*;

const WECHAT_INK: egui::Color32 = egui::Color32::from_rgb(23, 36, 59);
const WECHAT_MUTED: egui::Color32 = egui::Color32::from_rgb(108, 124, 149);
const WECHAT_BLUE: egui::Color32 = egui::Color32::from_rgb(56, 104, 232);
const WECHAT_BORDER: egui::Color32 = egui::Color32::from_rgb(222, 230, 242);

fn wechat_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(12.0).color(if selected {
            WECHAT_BLUE
        } else {
            WECHAT_MUTED
        }))
        .fill(if selected {
            egui::Color32::from_rgb(236, 242, 255)
        } else {
            egui::Color32::WHITE
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if selected { WECHAT_BLUE } else { WECHAT_BORDER },
        ))
        .rounding(8.0)
        .min_size(egui::vec2(0.0, 27.0)),
    )
}

impl App {
    pub(super) fn is_wechat(&self) -> bool {
        matches!(self.mode, Mode::WechatArticle | Mode::WechatChannels)
    }

    pub(super) fn clear_wechat(&mut self) {
        self.wechat_article = None;
        self.wechat_video = None;
        self.wechat_preview_open = false;
    }

    pub(super) fn spawn_wechat_reader(&mut self) {
        let article = self.mode == Mode::WechatArticle;
        let valid = if article {
            crate::wechat_article::article_url(&self.link).is_some()
        } else {
            crate::wechat_channels::link(&self.link).is_some()
        };
        if !valid {
            self.status = if article {
                "请粘贴微信公众号文章链接（mp.weixin.qq.com/s/…），也支持整段分享文字"
            } else {
                "请粘贴视频号网页分享链接；仅有微信聊天卡片或口令时，暂不能直接提取"
            }
            .into();
            return;
        }
        self.clear_wechat();
        self.busy = true;
        self.video_progress = None;
        self.status = if article {
            "在官方窗口阅读全文；需要验证时请手动完成，再点击获取正文"
        } else {
            "在官方窗口打开并播放视频，按页面提示登录，再点击检测视频"
        }
        .into();
        let input = self.link.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let message = (|| -> Result<Option<String>> {
                let response = tempfile::NamedTempFile::new()?;
                let mut command = std::process::Command::new(std::env::current_exe()?);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let status = command
                    .arg(if article {
                        "--wechat-article"
                    } else {
                        "--wechat-channels"
                    })
                    .arg(&input)
                    .arg(response.path())
                    .status()?;
                match status.code() {
                    Some(0) => Ok(Some(std::fs::read_to_string(response.path())?)),
                    Some(2) => Ok(None),
                    _ => {
                        anyhow::bail!("微信页面未完成获取，请重新打开；登录或验证需在官方窗口完成")
                    }
                }
            })();
            if article {
                let result = message.and_then(|m| {
                    m.map(|s| crate::wechat_article::Article::from_message(&s))
                        .transpose()
                });
                let _ = tx.send(Msg::WechatArticleLoaded(result));
            } else {
                let result = message.and_then(|m| {
                    m.map(|s| crate::wechat_channels::Video::from_message(&s))
                        .transpose()
                });
                let _ = tx.send(Msg::WechatVideoLoaded(result));
            }
        });
    }

    fn save_wechat(&mut self) {
        let tx = self.tx.clone();
        let directory = self.output_dir.clone();
        if self.mode == Mode::WechatArticle {
            let Some(article) = self.wechat_article.clone() else {
                return;
            };
            let format = self.text_format;
            self.busy = true;
            self.status = "正在保存公众号正文与配图…".into();
            thread::spawn(move || {
                let _ = tx.send(Msg::WechatSaved(article.export(&directory, format)));
            });
        } else {
            let Some(video) = self.wechat_video.clone() else {
                return;
            };
            self.busy = true;
            self.video_progress = Some(0.0);
            self.status = "正在下载视频号视频，完成后检查能否播放…".into();
            thread::spawn(move || {
                let result = crate::wechat_channels::download(&video, &directory, &|progress| {
                    let _ = tx.send(Msg::VideoProgress(progress));
                });
                let _ = tx.send(Msg::WechatSaved(result));
            });
        }
    }

    pub(super) fn wechat_source_card(&mut self, ui: &mut egui::Ui) {
        let article_mode = self.mode == Mode::WechatArticle;
        let compact = ui.ctx().screen_rect().height() < 680.0;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if article_mode {
                    "公众号文章"
                } else {
                    "视频号视频"
                })
                .size(15.0)
                .strong()
                .color(WECHAT_INK),
            );
            ui.label(
                egui::RichText::new(if article_mode {
                    "阅读并核对全文后导出"
                } else {
                    "在官方网页检测可播放视频"
                })
                .size(11.0)
                .color(WECHAT_MUTED),
            );
        });
        ui.add_space(if compact { 6.0 } else { 10.0 });
        ui.label(
            egui::RichText::new(if article_mode {
                "文章链接或分享文字"
            } else {
                "视频号网页链接或分享文字"
            })
            .size(13.0)
            .strong()
            .color(WECHAT_INK),
        );
        let response = egui::Frame::default()
            .fill(egui::Color32::from_rgb(249, 251, 255))
            .stroke(egui::Stroke::new(1.0_f32, WECHAT_BORDER))
            .rounding(10.0)
            .inner_margin(egui::Margin::symmetric(12.0, 7.0))
            .show(ui, |ui| {
                ui.add_enabled(
                    !self.busy,
                    egui::TextEdit::singleline(&mut self.link)
                        .frame(false)
                        .desired_width(ui.available_width())
                        .margin(egui::vec2(2.0, 3.0))
                        .hint_text(if article_mode {
                            "https://mp.weixin.qq.com/s/…"
                        } else {
                            "https://channels.weixin.qq.com/… 或 https://weixin.qq.com/sph/…"
                        }),
                )
            })
            .inner;
        if response.changed() {
            if self.link_changed() {
                return;
            }
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.busy {
            self.spawn_wechat_reader();
        }
        ui.add_space(if compact { 6.0 } else { 10.0 });
        if article_mode {
            ui.add_enabled_ui(!self.busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("导出格式")
                            .size(12.0)
                            .color(WECHAT_MUTED),
                    );
                    for format in TextFormat::ALL {
                        let label = match format {
                            TextFormat::Txt => "TXT",
                            TextFormat::Markdown => "Markdown",
                            TextFormat::Word => "Word",
                            TextFormat::Pdf => "PDF",
                        };
                        if wechat_chip(ui, label, self.text_format == format).clicked() {
                            self.text_format = format;
                        }
                    }
                });
            });
            if let Some(article) = self.wechat_article.clone() {
                ui.add_space(if compact { 6.0 } else { 10.0 });
                let first: String = article.body.chars().take(75).collect();
                let last: String = article
                    .body
                    .chars()
                    .rev()
                    .take(75)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(247, 250, 255))
                    .stroke(egui::Stroke::new(1.0_f32, WECHAT_BORDER))
                    .rounding(11.0)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("已获取")
                                    .size(11.0)
                                    .strong()
                                    .color(WECHAT_BLUE),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&article.title)
                                        .size(13.0)
                                        .strong()
                                        .color(WECHAT_INK),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&article.title);
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "{} 字 · {} 张配图",
                                article.body.chars().count(),
                                article.images.len()
                            ))
                            .size(11.0)
                            .color(WECHAT_MUTED),
                        );
                        for (label, text) in [("开头", first), ("末尾", last)] {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{label}：{}",
                                        text.replace('\n', " ")
                                    ))
                                    .size(11.0)
                                    .color(WECHAT_MUTED),
                                )
                                .truncate(),
                            );
                        }
                        ui.horizontal(|ui| {
                            if secondary_button(ui, "查看全文", true) {
                                self.wechat_preview_open = true;
                            }
                            if secondary_button(ui, "重新获取", !self.busy) {
                                self.spawn_wechat_reader();
                            }
                        });
                    });
            }
        } else {
            ui.label(
                egui::RichText::new("支持网页可播放的普通视频；微信专属卡片、加密视频暂不支持。")
                    .size(11.0)
                    .color(WECHAT_MUTED),
            );
            if let Some(video) = self.wechat_video.clone() {
                ui.add_space(if compact { 6.0 } else { 10.0 });
                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(247, 250, 255))
                    .stroke(egui::Stroke::new(1.0_f32, WECHAT_BORDER))
                    .rounding(11.0)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("已检测")
                                    .size(11.0)
                                    .strong()
                                    .color(WECHAT_BLUE),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&video.title)
                                        .size(13.0)
                                        .strong()
                                        .color(WECHAT_INK),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&video.title);
                        });
                        if secondary_button(ui, "重新检测", !self.busy) {
                            self.spawn_wechat_reader();
                        }
                    });
            }
        }
        ui.add_space(if compact { 8.0 } else { 12.0 });
        let ready = if article_mode {
            self.wechat_article.is_some()
        } else {
            self.wechat_video.is_some()
        };
        let action = if self.busy {
            "正在处理…"
        } else if article_mode && ready {
            "确认导出正文与配图"
        } else if article_mode {
            "打开文章并获取正文"
        } else if ready {
            "确认下载视频"
        } else {
            "打开网页并检测视频"
        };
        if super::reference::wide_button(
            ui,
            action,
            ui.available_width(),
            !self.busy && !self.link.trim().is_empty(),
            if article_mode {
                super::reference::Symbol::Document
            } else {
                super::reference::Symbol::Play
            },
        ) {
            if ready {
                self.save_wechat();
            } else {
                self.spawn_wechat_reader();
            }
        }
        ui.add_space(if compact { 4.0 } else { 7.0 });
        ui.label(
            egui::RichText::new(if article_mode {
                "先核对全文，再导出文档；配图单独保存。图片内文字不做 OCR。"
            } else {
                "需要登录时在官方窗口扫码；下载完成后，可在本地转写中识别文字和说话人。"
            })
            .size(11.0)
            .color(WECHAT_MUTED),
        );
        // Errors remain visible in full rather than disappearing in a truncated footer.
        if self.status.contains("失败") || self.status.starts_with("请粘贴") {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(&self.status)
                    .small()
                    .color(egui::Color32::from_rgb(180, 50, 50)),
            );
        }
    }

    pub(super) fn wechat_preview(&mut self, ctx: &egui::Context) {
        if let Some(article) = self.wechat_article.clone() {
            egui::Window::new("微信公众号 · 全文预览")
                .open(&mut self.wechat_preview_open)
                .default_width(640.0)
                .show(ctx, |ui| {
                    ui.heading(&article.title);
                    egui::ScrollArea::vertical()
                        .max_height((ctx.screen_rect().height() - 180.0).max(150.0))
                        .show(ui, |ui| {
                            ui.label(&article.body);
                        });
                });
        }
    }
}
