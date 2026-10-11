use super::*;

const INK: egui::Color32 = egui::Color32::from_rgb(23, 36, 59);
const MUTED: egui::Color32 = egui::Color32::from_rgb(108, 124, 149);
const BORDER: egui::Color32 = egui::Color32::from_rgb(226, 232, 242);
const SOFT_BLUE: egui::Color32 = egui::Color32::from_rgb(234, 241, 255);
const BLUE: egui::Color32 = egui::Color32::from_rgb(56, 104, 232);

fn card() -> egui::Frame {
    egui::Frame::default()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .rounding(16.0)
        .inner_margin(egui::Margin::same(12.0))
}

fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).size(12.0).color(MUTED));
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).size(13.0).strong().color(INK));
    ui.add_space(4.0);
}

fn chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(12.0).color(if selected {
            BLUE
        } else {
            INK
        }))
        .fill(if selected {
            SOFT_BLUE
        } else {
            egui::Color32::from_rgb(248, 250, 253)
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if selected {
                egui::Color32::from_rgb(181, 206, 255)
            } else {
                BORDER
            },
        ))
        .rounding(9.0)
        .min_size(egui::vec2(58.0, 28.0)),
    )
}
fn choice<T: PartialEq>(ui: &mut egui::Ui, value: &mut T, option: T, label: impl Into<String>) {
    let label = label.into();
    if chip(ui, &label, *value == option).clicked() {
        *value = option;
    }
}

fn quiet_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).size(12.0).color(MUTED))
            .frame(false)
            .min_size(egui::vec2(68.0, 28.0)),
    )
    .clicked()
}

fn link_input(
    ui: &mut egui::Ui,
    link: &mut String,
    mode: Mode,
    xhs_video: bool,
    busy: bool,
    width: f32,
) -> egui::Response {
    let hint = match mode {
        Mode::Youtube => "粘贴 YouTube 视频或 Shorts 链接…",
        Mode::WebText => "在这里粘贴知乎链接…",
        Mode::Douyin => "粘贴抖音视频链接或整段分享文字…",
        Mode::Xhs if xhs_video => "粘贴小红书视频链接或整段分享文字…",
        Mode::Xhs => "粘贴小红书图文链接或整段分享文字…",
        Mode::BiliText => "粘贴 B站 opus 图文、动态或 cv 专栏链接…",
        _ => "在这里粘贴视频链接…",
    };
    ui.set_width(width);
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(248, 250, 253))
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .rounding(11.0)
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(link)
                    .frame(false)
                    .hint_text(hint)
                    .desired_width(ui.available_width())
                    .margin(egui::vec2(2.0, 4.0)),
            )
        })
        .inner
}

fn inline_fetch_button(ui: &mut egui::Ui, label: &str, enabled: bool, width: f32) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(label)
                .size(13.0)
                .strong()
                .color(if enabled { egui::Color32::WHITE } else { MUTED }),
        )
        .fill(if enabled {
            BLUE
        } else {
            egui::Color32::from_rgb(248, 250, 253)
        })
        .stroke(if enabled {
            egui::Stroke::NONE
        } else {
            egui::Stroke::new(1.0_f32, BORDER)
        })
        .rounding(10.0)
        .min_size(egui::vec2(width, 37.0)),
    )
    .on_hover_text(if enabled {
        "获取链接内容并预览"
    } else {
        "粘贴链接后可获取"
    })
    .clicked()
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_reference(ctx);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_preferences(true);
    }
}

impl App {
    fn source_ready_to_export(&self) -> bool {
        ((self.mode == Mode::BiliText || (self.mode == Mode::Douyin && self.douyin_article_mode))
            && self.douyin_article.is_some())
            || (self.mode == Mode::WebText && self.zhihu_content.is_some())
    }

    fn fetch_link_content(&mut self) {
        if (self.mode == Mode::Douyin && self.douyin_article_mode) || self.mode == Mode::BiliText {
            self.spawn_read_douyin_article();
        } else if self.mode == Mode::WebText {
            self.spawn_fetch_zhihu();
        } else {
            self.spawn_fetch_video();
        }
    }

    pub(super) fn source_card(&mut self, ui: &mut egui::Ui) {
        if self.mode == Mode::Twitter {
            self.twitter_source_card(ui);
            return;
        }
        if self.is_wechat() {
            self.wechat_source_card(ui);
            return;
        }
        let compact = ui.ctx().screen_rect().height() < 680.0;
        let source_frame = egui::Frame::default()
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(1.0_f32, BORDER))
            .rounding(16.0)
            .inner_margin(egui::Margin::same(if compact { 8.0 } else { 14.0 }));
        source_frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            if compact {
                ui.spacing_mut().item_spacing.y = 4.0;
            }
            if (self.mode == Mode::Douyin && self.douyin_article_mode)
                || self.mode == Mode::BiliText
            {
                ui.spacing_mut().item_spacing.y = 3.0;
            }
            let before = (
                self.mode,
                self.douyin_article_mode,
                self.gallery_mode,
                self.xhs_video,
                self.zhihu_article,
            );
            ui.add_enabled_ui(!self.busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.mode != Mode::Transcribe {
                        ui.label(
                            egui::RichText::new("内容类型")
                                .size(12.0)
                                .strong()
                                .color(INK),
                        );
                    }
                    match self.mode {
                        Mode::Subtitle | Mode::Video | Mode::BiliText => {
                            let video = self.mode != Mode::BiliText;
                            if chip(ui, "视频", video).clicked() && !video {
                                self.mode = Mode::Video;
                            }
                            if chip(ui, "图文", !video).clicked() {
                                self.mode = Mode::BiliText;
                            }
                            if video {
                                ui.add_space(6.0);
                                choice(ui, &mut self.mode, Mode::Video, "下载视频");
                                choice(ui, &mut self.mode, Mode::Subtitle, "提取字幕");
                            }
                        }
                        Mode::Douyin => {
                            if chip(ui, "视频", !self.douyin_article_mode && !self.gallery_mode)
                                .clicked()
                            {
                                self.douyin_article_mode = false;
                                self.gallery_mode = false;
                            }
                            if chip(ui, "图文", !self.douyin_article_mode && self.gallery_mode)
                                .clicked()
                            {
                                self.douyin_article_mode = false;
                                self.gallery_mode = true;
                            }
                            if chip(ui, "长文章", self.douyin_article_mode).clicked() {
                                self.douyin_article_mode = true;
                            }
                        }
                        Mode::Youtube => {
                            choice(ui, &mut self.youtube_subtitles, false, "下载视频");
                            choice(ui, &mut self.youtube_subtitles, true, "提取字幕");
                        }
                        Mode::Xhs => {
                            choice(ui, &mut self.xhs_video, false, "图文");
                            choice(ui, &mut self.xhs_video, true, "视频");
                        }
                        Mode::WebText => {
                            choice(ui, &mut self.zhihu_article, true, "专栏");
                            choice(ui, &mut self.zhihu_article, false, "问答");
                        }
                        _ => {}
                    }
                    if self.mode != Mode::Transcribe {
                        ui.add_space(6.0);
                        if quiet_button(ui, "检测评论", !self.busy) {
                            self.spawn_comments();
                        }
                        if self.comments.is_some() && quiet_button(ui, "选择评论", !self.busy) {
                            self.comment_window_open = true;
                        }
                    }
                });
            });
            if before
                != (
                    self.mode,
                    self.douyin_article_mode,
                    self.gallery_mode,
                    self.xhs_video,
                    self.zhihu_article,
                )
            {
                self.external = None;
                self.video = None;
                self.douyin_article = None;
                self.zhihu_content = None;
                self.status = "已切换内容类型，请重新获取".into();
            }
            if self.mode == Mode::Transcribe {
                self.file_drop_zone(ui);
            } else {
                section(ui, "内容链接");
                let ready_to_export = self.source_ready_to_export();
                let fetched = match self.mode {
                    Mode::Youtube => self.youtube.is_some(),
                    _ => self.video.is_some() || self.external.is_some(),
                };
                let button_label = if self.busy {
                    "处理中…"
                } else if fetched {
                    "重新获取"
                } else {
                    "获取"
                };
                let button_width = 106.0;
                let button_enabled = !self.busy && !self.link.trim().is_empty();
                let (response, fetch_clicked) = if !ready_to_export && ui.available_width() >= 430.0
                {
                    ui.horizontal(|ui| {
                        let field_width =
                            ui.available_width() - button_width - ui.spacing().item_spacing.x;
                        let response = ui
                            .allocate_ui_with_layout(
                                egui::vec2(field_width, 0.0),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    link_input(
                                        ui,
                                        &mut self.link,
                                        self.mode,
                                        self.xhs_video,
                                        self.busy,
                                        field_width,
                                    )
                                },
                            )
                            .inner;
                        let clicked =
                            inline_fetch_button(ui, button_label, button_enabled, button_width);
                        (response, clicked)
                    })
                    .inner
                } else {
                    let width = ui.available_width();
                    let response = link_input(
                        ui,
                        &mut self.link,
                        self.mode,
                        self.xhs_video,
                        self.busy,
                        width,
                    );
                    let clicked = if ready_to_export {
                        false
                    } else {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            inline_fetch_button(ui, button_label, button_enabled, button_width)
                        })
                        .inner
                    };
                    (response, clicked)
                };
                if response.changed() {
                    if self.link_changed() {
                        return;
                    }
                }
                let enter_pressed =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (fetch_clicked || enter_pressed) && !self.busy && !self.link.trim().is_empty() {
                    self.fetch_link_content();
                }
            }

            if self.mode == Mode::Subtitle {
                ui.add_space(6.0);
                let before = self.bili_local_transcribe;
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("识别方式")
                                .size(12.0)
                                .strong()
                                .color(INK),
                        );
                        choice(
                            ui,
                            &mut self.bili_local_transcribe,
                            false,
                            "直接获取 AI / 原字幕",
                        );
                        choice(
                            ui,
                            &mut self.bili_local_transcribe,
                            true,
                            "下载音频 · 本地识别",
                        );
                    });
                });
                if before != self.bili_local_transcribe {
                    self.video = None;
                    self.tracks.clear();
                    self.streams.clear();
                    self.status = "已切换识别方式，请重新获取视频信息".into();
                }
            }
            if (!matches!(self.mode, Mode::Video | Mode::Douyin)
                || ((self.mode == Mode::Douyin && self.douyin_article_mode)
                    || self.mode == Mode::BiliText)
                || self
                    .external
                    .as_ref()
                    .is_some_and(|video| video.gallery.is_some()))
                && (self.mode != Mode::WebText || self.zhihu_content.is_some())
                && !(self.mode == Mode::Xhs && self.xhs_video)
                && !(self.mode == Mode::Youtube && !self.youtube_subtitles)
            {
                ui.add_space(6.0);

                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("导出格式")
                                .size(12.0)
                                .strong()
                                .color(INK),
                        );
                        for format in TextFormat::ALL {
                            let (label, detail) = match format {
                                TextFormat::Txt => ("TXT", "纯文本，适合复制和编辑"),
                                TextFormat::Markdown => ("Markdown", "保留文字段落与图片链接"),
                                TextFormat::Word => ("Word", "可在 Word / WPS 中编辑"),
                                TextFormat::Pdf => ("PDF", "适合阅读和分享"),
                            };
                            if chip(ui, label, self.text_format == format)
                                .on_hover_text(detail)
                                .clicked()
                            {
                                self.text_format = format;
                            }
                        }
                    });
                });
            }

            if self.mode == Mode::WebText {
                ui.add_space(6.0);
                if !self.zhihu_article {
                    let previous_scope = self.zhihu_all_answers;
                    ui.add_enabled_ui(!self.busy, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new("获取范围")
                                    .size(12.0)
                                    .strong()
                                    .color(INK),
                            );
                            choice(
                                ui,
                                &mut self.zhihu_all_answers,
                                true,
                                "整个问题 · 包含更多回答",
                            );
                            choice(ui, &mut self.zhihu_all_answers, false, "仅链接中的回答");
                        });
                    });
                    if previous_scope != self.zhihu_all_answers {
                        self.zhihu_content = None;
                    }
                }
                if let Some(content) = self.zhihu_content.clone() {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "已获取 {} 条{}",
                                content.count(),
                                if content.is_question {
                                    "回答"
                                } else {
                                    "内容"
                                }
                            ))
                            .strong()
                            .size(18.0)
                            .color(ACCENT_DARK),
                        );
                        if secondary_button(ui, "重新获取", !self.busy) {
                            self.spawn_fetch_zhihu();
                        }
                    });

                    if content.total > content.count() {
                        caption(
                            ui,
                            format!(
                                "知乎显示共 {} 条，当前可获取 {} 条。",
                                content.total,
                                content.count()
                            ),
                        );
                    }
                    if content.is_question {
                        ui.add_space(8.0);
                        ui.add_enabled_ui(!self.busy, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("导出范围：从");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.zhihu_from)
                                        .desired_width(85.0),
                                );
                                ui.label("到");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.zhihu_to)
                                        .desired_width(85.0),
                                );
                                ui.label(format!("条（共 {} 条）", content.count()));
                            });
                        });
                    }
                }
            } else if self.mode == Mode::Transcribe
                || self.mode == Mode::Subtitle && self.bili_local_transcribe
            {
                ui.add_space(6.0);
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("识别语言");
                        egui::ComboBox::from_id_salt("transcribe_language")
                            .selected_text(match self.transcribe_language.as_str() {
                                "zh" => "中文",
                                "en" => "English",
                                _ => "自动识别",
                            })
                            .show_ui(ui, |ui| {
                                for (value, label) in
                                    [("auto", "自动识别"), ("zh", "中文"), ("en", "English")]
                                {
                                    choice(ui, &mut self.transcribe_language, value.into(), label);
                                }
                            });
                        ui.checkbox(&mut self.speaker_diarization, "区分说话人")
                            .on_hover_text("说话人编号不代表真实身份");
                        if self.speaker_diarization {
                            egui::ComboBox::from_id_salt("speaker_count")
                                .width(95.0)
                                .selected_text(if self.speaker_count == 0 {
                                    "人数：自动".into()
                                } else {
                                    format!("{} 人", self.speaker_count)
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.speaker_count, 0, "自动判断人数");
                                    for count in 1..=12 {
                                        ui.selectable_value(
                                            &mut self.speaker_count,
                                            count,
                                            format!("{count} 人"),
                                        );
                                    }
                                });
                        }
                    });
                });
            }

            ui.add_space(6.0);
            if (self.mode == Mode::Douyin && self.douyin_article_mode)
                || self.mode == Mode::BiliText
            {
                if let Some(article) = self.douyin_article.clone() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "{} · {} 字 · {} 张配图",
                                article.title,
                                article.body.chars().count(),
                                article.images.len()
                            ))
                            .strong(),
                        )
                        .truncate(),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "开头：{}",
                                article
                                    .body
                                    .chars()
                                    .take(90)
                                    .collect::<String>()
                                    .replace('\n', " ")
                            ))
                            .size(12.0)
                            .color(MUTED),
                        )
                        .truncate(),
                    );
                    let tail = article
                        .body
                        .chars()
                        .rev()
                        .take(90)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<String>();
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("末尾：{}", tail.replace('\n', " ")))
                                .size(12.0)
                                .color(MUTED),
                        )
                        .truncate(),
                    );
                    ui.horizontal(|ui| {
                        if secondary_button(ui, "重新获取正文", !self.busy) {
                            self.spawn_read_douyin_article();
                        }
                        if secondary_button(ui, "查看全部文字", true) {
                            self.douyin_preview_open = true;
                        }
                    });
                    caption(
                        ui,
                        if article.selected {
                            "这是选中文字，请核对范围后导出。"
                        } else {
                            "请与网页核对正文是否完整；图片中的文字暂不支持。"
                        },
                    );
                }
            }
            if self.mode == Mode::Transcribe || self.source_ready_to_export() {
                ui.add_space(6.0);
                let enabled = !self.busy
                    && if self.mode == Mode::Transcribe {
                        self.media_file.is_some() && self.ffmpeg_ok
                    } else {
                        !self.link.trim().is_empty()
                    };
                let action = match self.mode {
                    Mode::Transcribe => "开始识别",
                    Mode::BiliText => "确认导出图文",
                    Mode::Douyin => "确认导出文章",
                    Mode::WebText => "确认导出",
                    _ => unreachable!(),
                };
                if super::reference::wide_button(
                    ui,
                    if self.busy { "正在处理…" } else { action },
                    ui.available_width(),
                    enabled,
                    super::reference::Symbol::Play,
                ) {
                    match self.mode {
                        Mode::BiliText | Mode::Douyin => self.spawn_export_douyin_article(),
                        Mode::Transcribe => self.spawn_transcribe(),
                        Mode::WebText => self.spawn_export_zhihu(),
                        _ => unreachable!(),
                    }
                }
            }
            // Keep a platform switch near the input if a link was pasted in the wrong workspace.
            if (self.status.contains("YouTube")
                || self.status.contains("抖音")
                || self.status.contains("小红书")
                || self.status.starts_with("解析链接失败"))
                && !(self.mode == Mode::Douyin
                    && self.douyin_article_mode
                    && self.douyin_article.is_some())
            {
                ui.add_space(4.0);

                if self.mode == Mode::Subtitle && external::is_supported(&self.link) {
                    let xhs = external::is_xhs(&self.link);
                    let youtube = crate::youtube::link(&self.link).is_some();
                    if secondary_button(
                        ui,
                        if youtube {
                            "切换到 YouTube"
                        } else if xhs {
                            "切换到小红书图文"
                        } else {
                            "切换到抖音下载"
                        },
                        !self.busy,
                    ) {
                        self.mode = if youtube {
                            Mode::Youtube
                        } else if xhs {
                            Mode::Xhs
                        } else {
                            Mode::Douyin
                        };
                        self.status = if youtube {
                            "已切换到 YouTube，请获取视频与字幕列表"
                        } else if xhs {
                            "已切换到小红书图文，点击获取读取笔记"
                        } else {
                            "已切换到抖音下载，点击获取尝试解析"
                        }
                        .into();
                    }
                }
            }
            if !compact {
                ui.add_space(3.0);
            }
            if self.mode == Mode::Transcribe && !self.ffmpeg_ok {
                ui.label(
                    egui::RichText::new("音视频处理组件未就绪，请检查应用安装。 ")
                        .color(egui::Color32::from_rgb(181, 115, 26)),
                );
            }
        });
    }

    pub(super) fn media_card(&mut self, ui: &mut egui::Ui) {
        if self.mode == Mode::Twitter {
            return;
        }
        if self.is_wechat() {
            return;
        }
        if matches!(self.mode, Mode::Transcribe | Mode::WebText | Mode::BiliText) {
            return;
        }
        if self.mode == Mode::Douyin && (self.douyin_article_mode || self.external.is_none()) {
            return;
        }
        if self.mode == Mode::Youtube && self.youtube_subtitles {
            if let Some(video) = self.youtube.clone() {
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if ui.ctx().screen_rect().height() >= 680.0 {
                        section(ui, "字幕已就绪");
                    }
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&video.title)
                                .size(15.0)
                                .strong()
                                .color(INK),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&video.title);
                    if video.tracks.is_empty() {
                        caption(ui, "没有可用字幕；可下载视频后使用音视频转写。");
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("字幕语言");
                            egui::ComboBox::from_id_salt("youtube_language")
                                .width((ui.available_width() - 150.0).max(180.0))
                                .selected_text(
                                    video
                                        .tracks
                                        .get(self.youtube_track)
                                        .map(|t| t.label.as_str())
                                        .unwrap_or("请选择"),
                                )
                                .show_ui(ui, |ui| {
                                    for (i, track) in video.tracks.iter().enumerate() {
                                        choice(ui, &mut self.youtube_track, i, &track.label);
                                    }
                                });
                        });
                        if primary_button(ui, "导出字幕与 SRT", !self.busy) {
                            self.spawn_export_youtube();
                        }
                    }
                });
            }
            return;
        }
        if let Some(video) = self.external.clone().filter(|video| {
            matches!(
                self.mode,
                Mode::Video | Mode::Douyin | Mode::Xhs | Mode::Youtube
            ) && (self.mode != Mode::Xhs || external::is_xhs(&video.url))
                && (self.mode != Mode::Douyin || external::is_douyin(&video.url))
        }) {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                let is_gallery = video.gallery.is_some();
                section(
                    ui,
                    if is_gallery {
                        "图文已就绪"
                    } else {
                        "视频已就绪"
                    },
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&video.title)
                            .size(15.0)
                            .strong()
                            .color(INK),
                    )
                    .truncate(),
                )
                .on_hover_text(&video.title);
                ui.add_space(3.0);
                if let Some(gallery) = &video.gallery {
                    caption(
                        ui,
                        format!(
                            "{} 张图片 · 文案导出为 {}",
                            gallery.images.len(),
                            self.text_format.label()
                        ),
                    );
                }
                if primary_button(
                    ui,
                    if is_gallery {
                        "保存图片和文案"
                    } else {
                        "下载视频"
                    },
                    !self.busy,
                ) {
                    self.spawn_download_external();
                }
            });
            ui.add_space(6.0);
            return;
        }
        let Some((title, bvid, pages)) = self
            .video
            .as_ref()
            .map(|video| (video.title.clone(), video.bvid.clone(), video.pages.clone()))
        else {
            return;
        };
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let _ = bvid;
            if ui.ctx().screen_rect().height() >= 680.0 {
                section(ui, "视频已就绪");
            }
            ui.add(
                egui::Label::new(egui::RichText::new(&title).size(15.0).strong().color(INK))
                    .truncate(),
            )
            .on_hover_text(&title);
            ui.add_space(4.0);
            if pages.len() > 1 {
                let previous_page = self.selected_page;
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("分集");
                        let label = pages
                            .iter()
                            .find(|page| page.page == self.selected_page)
                            .map(|page| format!("P{} {}", page.page, page.part))
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("page_select")
                            .width((ui.available_width() - 20.0).min(420.0))
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                for page in &pages {
                                    choice(
                                        ui,
                                        &mut self.selected_page,
                                        page.page,
                                        format!("P{} {}", page.page, page.part),
                                    );
                                }
                            });
                    });
                });
                if previous_page != self.selected_page {
                    self.spawn_fetch_tracks(self.selected_page);
                }
                ui.add_space(8.0);
            }
            if self.mode == Mode::Video {
                ui.add_enabled_ui(!self.busy && !self.streams.is_empty(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("画质");
                        egui::ComboBox::from_id_salt("stream_select")
                            .selected_text(
                                self.streams
                                    .get(self.selected_stream)
                                    .map(|stream| stream.label.clone())
                                    .unwrap_or_else(|| "暂无可用画质".into()),
                            )
                            .show_ui(ui, |ui| {
                                for (index, stream) in self.streams.iter().enumerate() {
                                    choice(ui, &mut self.selected_stream, index, &stream.label);
                                }
                            });
                    });
                });
                ui.add_space(6.0);
                if primary_button(ui, "下载 MP4", !self.busy && !self.streams.is_empty()) {
                    self.spawn_download_video();
                }
                if !self.ffmpeg_ok {
                    caption(ui, "音视频合并组件未就绪，部分画质暂不可下载。");
                }
            } else if self.mode == Mode::Subtitle && self.bili_local_transcribe {
                if primary_button(ui, "下载音频并本地识别", !self.busy && self.ffmpeg_ok) {
                    self.spawn_bili_transcribe();
                }
            } else if !self.tracks.is_empty() {
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("字幕语言");
                        egui::ComboBox::from_id_salt("track_select")
                            .selected_text(
                                self.tracks
                                    .get(self.selected_track)
                                    .map(|track| track.lan_doc.clone())
                                    .unwrap_or_default(),
                            )
                            .show_ui(ui, |ui| {
                                for (index, track) in self.tracks.iter().enumerate() {
                                    choice(ui, &mut self.selected_track, index, &track.lan_doc);
                                }
                            });
                    });
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if primary_button(
                        ui,
                        &format!("导出 {}", self.text_format.label()),
                        !self.busy,
                    ) {
                        self.spawn_download("txt");
                    }
                    if secondary_button(ui, "下载 SRT 字幕", !self.busy) {
                        self.spawn_download("srt");
                    }
                });
            } else if !self.busy {
                caption(ui, "暂未找到字幕，可以尝试登录或切换到视频下载。");
            }
        });
        ui.add_space(6.0);
    }

    pub(super) fn result_card(&mut self, ui: &mut egui::Ui) {
        if self.status.trim().is_empty()
            || !self.busy
                && matches!(
                    self.status.as_str(),
                    "粘贴B站视频链接，然后点「获取」"
                        | "粘贴内容链接，自动识别平台；获取后预览并保存"
                        | "已切换内容类型，请重新获取"
                        | "文件已就绪，选择格式后点击开始识别"
                )
        {
            self.reveal_result = false;
            return;
        }
        let compact = ui.ctx().screen_rect().height() < 680.0;
        let is_error = ["失败", "错误", "过期", "未登录", "没有"]
            .iter()
            .any(|word| self.status.contains(word));
        let is_success = ["已保存", "成功", "导出完成", "已获取", "获取到"]
            .iter()
            .any(|word| self.status.contains(word));
        let (surface, outline) = if self.busy {
            (
                egui::Color32::from_rgb(244, 248, 255),
                egui::Color32::from_rgb(207, 222, 249),
            )
        } else if is_error {
            (
                egui::Color32::from_rgb(255, 248, 248),
                egui::Color32::from_rgb(243, 218, 221),
            )
        } else if is_success {
            (
                egui::Color32::from_rgb(246, 251, 249),
                egui::Color32::from_rgb(209, 233, 221),
            )
        } else {
            (egui::Color32::from_rgb(247, 249, 253), BORDER)
        };
        ui.add_space(if compact { 3.0 } else { 8.0 });
        egui::Frame::default()
            .fill(surface)
            .stroke(egui::Stroke::new(1.0_f32, outline))
            .rounding(11.0)
            .inner_margin(egui::Margin::symmetric(
                10.0,
                if compact { 5.0 } else { 7.0 },
            ))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.spinner();
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::hover());
                        super::reference::icon(
                            ui.painter(),
                            rect,
                            super::reference::Symbol::Info,
                            status_color(&self.status),
                        );
                    }
                    let message = if self.status.starts_with("已保存:")
                        || self.status.contains("导出完成")
                    {
                        "保存完成，可从左侧最近文件直接打开".into()
                    } else {
                        self.status.clone()
                    };
                    ui.add_sized(
                        [(ui.available_width() - 50.0).max(80.0), 22.0],
                        egui::Label::new(egui::RichText::new(&message).size(12.0).color(INK))
                            .truncate(),
                    )
                    .on_hover_text(&self.status);
                    if ui.add(egui::Button::new("详情").frame(false)).clicked() {
                        self.status_details_open = true;
                    }
                });
                if let Some(progress) = self.video_progress.filter(|_| self.busy) {
                    ui.add(
                        egui::ProgressBar::new(progress as f32)
                            .desired_width(ui.available_width())
                            .show_percentage(),
                    );
                }
            });
        self.reveal_result = false;
    }

    pub(super) fn login_window(&mut self, ctx: &egui::Context) {
        let Some(stage) = self.qr_stage.clone() else {
            return;
        };
        let mut open = true;
        let mut cancel = false;
        egui::Window::new("B站扫码登录")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    caption(ui, "使用 B站 App 扫描二维码");
                    ui.add_space(6.0);
                    if let Some(texture) = &self.qr_texture {
                        ui.add(
                            egui::Image::new(texture).fit_to_exact_size(egui::vec2(224.0, 224.0)),
                        );
                    }
                    ui.add_space(6.0);
                    ui.label(stage);
                    ui.add_space(8.0);
                    cancel = secondary_button(ui, "取消登录", true);
                });
            });
        if cancel || !open {
            self.qr_cancelled.store(true, Ordering::Relaxed);
            self.qr_stage = None;
            self.qr_texture = None;
            self.busy = false;
            self.status = "已取消扫码登录".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::icon_data::IconDataExt;
    use std::collections::HashMap;

    #[test]
    fn all_workspaces_render_at_normal_and_compact_sizes() {
        for size in [
            [1000.0, 760.0],
            [940.0, 640.0],
            [900.0, 560.0],
            [780.0, 600.0],
            [780.0, 540.0],
        ] {
            for (index, mode) in [
                Mode::Subtitle,
                Mode::Video,
                Mode::Transcribe,
                Mode::WebText,
                Mode::WebText,
                Mode::Transcribe,
                Mode::Douyin,
                Mode::Douyin,
                Mode::Xhs,
                Mode::Youtube,
                Mode::Youtube,
                Mode::BiliText,
                Mode::Xhs,
                Mode::WebText,
                Mode::Subtitle,
                Mode::WechatArticle,
                Mode::WechatChannels,
                Mode::WechatArticle,
                Mode::WechatChannels,
                Mode::Twitter,
                Mode::Twitter,
                Mode::Twitter,
            ]
            .into_iter()
            .enumerate()
            {
                let ctx = egui::Context::default();
                load_chinese_font(&ctx);
                apply_style(&ctx);
                let mut app = App::new(&ctx);
                app.mode = mode;
                if index >= 20 {
                    app.twitter_video_mode = index == 21;
                    app.twitter_post = Some(Arc::new(crate::twitter::Post {
                        title: "一条有较长标题的 X 帖子，用于核对小窗口中的主要操作和预览布局"
                            .into(),
                        body: "完整正文测试。\n".repeat(100),
                        author: "Example Author @example".into(),
                        source: "https://x.com/example/status/123456".into(),
                        published_at: "2026-10-11".into(),
                        images: vec!["https://pbs.twimg.com/media/example.jpg".into()],
                        video_count: 1,
                        warning: Some(
                            "当前为公开预览，长文和部分媒体可能省略；可在官方页面展开后读取。"
                                .into(),
                        ),
                    }));
                }
                if index == 17 {
                    app.status = "已获取公众号正文 1000 字、3 张配图，核对后确认导出".into();
                    app.saved_files = vec![
                        PathBuf::from("公众号文章与配图"),
                        PathBuf::from("另一个最近导出的文件.pdf"),
                    ];
                    app.wechat_article = Some(Arc::new(crate::wechat_article::Article {
                        title:
                            "公众号文章：保持完整的文字与图片，较长的标题也能在小窗口里显示主要操作"
                                .into(),
                        body: "正文第一段，用于检查公众号预览。\n".repeat(100),
                        source: "https://mp.weixin.qq.com/s/Example".into(),
                        selected: false,
                        images: vec!["https://mmbiz.qpic.cn/example.png".into(); 3],
                        author: "示例作者".into(),
                        published_at: "2026-10-07".into(),
                        native_pdf: None,
                    }));
                }
                if index == 18 {
                    app.wechat_video = Some(crate::wechat_channels::Video {
                        title: "已获取的视频号视频".into(),
                        source: "https://channels.weixin.qq.com/web/pages/feed?oid=example".into(),
                        media_url: "https://finder.video.qq.com/example.mp4".into(),
                    });
                }
                if index == 14 {
                    app.bili_local_transcribe = true;
                }
                if mode == Mode::Youtube {
                    app.youtube_subtitles = index == 10;
                    app.youtube = Some(crate::youtube::Video {
                        title: "YouTube 示例".into(),
                        url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".into(),
                        tracks: vec![crate::youtube::Track {
                            language: "zh-Hans".into(),
                            label: "中文 · 自动字幕".into(),
                            automatic: true,
                        }],
                    });
                }
                if index == 10 {
                    app.xhs_video = true;
                }
                if index == 11 {
                    app.zhihu_article = true;
                }
                if index == 7 {
                    app.douyin_article_mode = true;
                    app.douyin_article = Some(Arc::new(crate::douyin_article::Article {
                        title: "抖音长文章预览".into(),
                        body: "这是保留分段的文章正文，用于检查预览与导出功能。\n".repeat(100),
                        source: "https://www.douyin.com/note/123".into(),
                        selected: false,
                        images: vec![],
                    }));
                    app.status = "已获取抖音文字，请核对正文与末尾后确认导出".into();
                }
                if index == 4 {
                    app.status = "已保存: 界面验证文件.txt".into();
                    app.reveal_result = true;
                    app.saved_files = vec![
                        PathBuf::from("界面验证文件.txt"),
                        PathBuf::from("界面验证文件.srt"),
                    ];
                }
                if index == 3 {
                    app.zhihu_content = Some(Arc::new(zhihu::ZhihuContent {
                        title: "已获取的知乎问题".into(),
                        sections: vec!["回答正文".into(); 22],
                        total: 22,
                        is_question: true,
                    }));
                    app.zhihu_from = "1".into();
                    app.zhihu_to = "22".into();
                }
                app.ffmpeg_ok = true;
                app.link = "https://www.bilibili.com/video/BV1example".into();
                app.media_file = Some(PathBuf::from(
                    "一份名字较长的音视频文件用于验证界面布局.mp4",
                ));
                if index == 5 {
                    app.media_file = None;
                }
                app.video = Some(VideoInfo {
                    aid: 1,
                    bvid: "BV1example".into(),
                    title: "学习与记录：把值得保留的内容变成文字".into(),
                    pages: vec![crate::bili::PageInfo {
                        page: 1,
                        part: "第一集".into(),
                        cid: 1,
                    }],
                });
                app.tracks = vec![SubTrack {
                    lan: "zh".into(),
                    lan_doc: "中文（自动生成）".into(),
                    url: String::new(),
                }];
                app.streams = vec![VideoStream {
                    quality_id: 80,
                    label: "1080P 高清".into(),
                    video_url: String::new(),
                    audio_url: None,
                }];
                let mut textures: HashMap<egui::TextureId, egui::ColorImage> = HashMap::new();
                let mut output = None;
                for frame_index in 0..3 {
                    let frame = ctx.run(
                        egui::RawInput {
                            time: Some(frame_index as f64 * 0.5),
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(size[0], size[1]),
                            )),
                            ..Default::default()
                        },
                        |ctx| app.render_reference(ctx),
                    );
                    for (id, delta) in &frame.textures_delta.set {
                        let image = match &delta.image {
                            egui::ImageData::Color(image) => (**image).clone(),
                            egui::ImageData::Font(image) => egui::ColorImage {
                                size: image.size,
                                pixels: image.srgba_pixels(None).collect(),
                            },
                        };
                        if let Some([x, y]) = delta.pos {
                            let texture = textures.get_mut(id).unwrap();
                            for row in 0..image.size[1] {
                                let offset = (y + row) * texture.size[0] + x;
                                texture.pixels[offset..offset + image.size[0]].copy_from_slice(
                                    &image.pixels[row * image.size[0]..(row + 1) * image.size[0]],
                                );
                            }
                        } else {
                            textures.insert(*id, image);
                        }
                    }
                    output = Some(frame);
                }
                let output = output.unwrap();
                assert!(
                    ctx.data(|data| data.get_temp::<bool>(egui::Id::new("workspace_fits")))
                        .unwrap_or(false),
                    "工作区溢出：size={size:?}, mode={index}"
                );
                if index == 4 && !app.saved_files.is_empty() {
                    assert!(
                        output.shapes.iter().any(|shape| {
                            if let egui::epaint::Shape::Text(text) = &shape.shape {
                                text.galley.text().contains("界面验证")
                                    && shape.clip_rect.contains(text.pos)
                            } else {
                                false
                            }
                        }),
                        "Recent saved file must remain visible in the sidebar"
                    );
                }
                assert!(
                    ctx.available_rect().width() > 500.0,
                    "Main workspace is too narrow"
                );
                assert!(
                    ctx.available_rect().height() > 200.0,
                    "Main workspace is too short"
                );
                let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                for primitive in &primitives {
                    assert!(primitive.clip_rect.is_finite());
                }
                if let Ok(destination) = std::env::var("SHIWEN_UI_SNAPSHOT_DIR") {
                    let folder =
                        PathBuf::from(destination).join(format!("{}-{index}", size[0] as u32));
                    std::fs::create_dir_all(&folder).unwrap();
                    for (id, image) in textures {
                        let rgba = image
                            .pixels
                            .iter()
                            .flat_map(|color| color.to_array())
                            .collect();
                        let icon = egui::IconData {
                            width: image.size[0] as u32,
                            height: image.size[1] as u32,
                            rgba,
                        };
                        std::fs::write(
                            folder.join(format!("{id:?}.png")),
                            icon.to_png_bytes().unwrap(),
                        )
                        .unwrap();
                    }
                    let meshes: Vec<_> = primitives.into_iter().filter_map(|primitive| {
                        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else { return None; };
                        Some(serde_json::json!({ "clip": [primitive.clip_rect.min.x, primitive.clip_rect.min.y, primitive.clip_rect.max.x, primitive.clip_rect.max.y], "texture": format!("{:?}.png", mesh.texture_id), "indices": mesh.indices, "vertices": mesh.vertices.iter().map(|vertex| serde_json::json!([vertex.pos.x, vertex.pos.y, vertex.uv.x, vertex.uv.y, vertex.color.r(), vertex.color.g(), vertex.color.b(), vertex.color.a()])).collect::<Vec<_>>() }))
                    }).collect();
                    std::fs::write(
                        folder.join("scene.json"),
                        serde_json::to_vec(&serde_json::json!({"size": size, "meshes": meshes}))
                            .unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
}
