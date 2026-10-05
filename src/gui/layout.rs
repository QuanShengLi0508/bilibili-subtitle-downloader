use super::*;

const INK: egui::Color32 = egui::Color32::from_rgb(20, 36, 73);
const MUTED: egui::Color32 = egui::Color32::from_rgb(119, 139, 173);
const BORDER: egui::Color32 = egui::Color32::from_rgb(226, 233, 243);
const SOFT_BLUE: egui::Color32 = egui::Color32::from_rgb(235, 243, 255);

fn card() -> egui::Frame {
    egui::Frame::default()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .rounding(14.0)
        .inner_margin(egui::Margin::same(10.0))
}

fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).size(12.0).color(MUTED));
}

fn section(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.label(egui::RichText::new(title).size(15.0).strong().color(INK));
    let _ = detail;
    ui.add_space(4.0);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_reference(ctx);
    }
}

impl App {
    pub(super) fn source_card(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if self.mode == Mode::Transcribe {
                self.file_drop_zone(ui);
            } else {
                section(
                    ui,
                    "粘贴内容链接",
                    match self.mode {
                        Mode::Subtitle => "支持 B站视频链接、短链接或 BV / AV 号",
                        Mode::Video => "支持 B站、抖音、小红书链接",
                        Mode::Douyin => "支持抖音短链接、精选链接和整段分享文字",
                        _ => "支持知乎回答、专栏和问题链接",
                    },
                );
                let response = ui.add_enabled(
                    !self.busy,
                    egui::TextEdit::singleline(&mut self.link)
                        .hint_text(if self.mode == Mode::WebText {
                            "在这里粘贴知乎链接…"
                        } else if self.mode == Mode::Douyin {
                            "粘贴抖音视频链接或整段分享文字…"
                        } else {
                            "在这里粘贴视频链接…"
                        })
                        .desired_width(ui.available_width())
                        .margin(egui::vec2(10.0, 8.0)),
                );
                if response.changed() {
                    self.zhihu_content = None;
                    self.video = None;
                    self.external = None;
                    self.tracks.clear();
                    self.streams.clear();
                }
                if response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    && !self.busy
                    && !self.link.trim().is_empty()
                {
                    if self.mode == Mode::WebText {
                        self.spawn_fetch_zhihu();
                    } else {
                        self.spawn_fetch_video();
                    }
                }
            }

            if !matches!(self.mode, Mode::Video | Mode::Douyin)
                && (self.mode != Mode::WebText || self.zhihu_content.is_some())
            {
                ui.add_space(6.0);

                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for format in TextFormat::ALL {
                            let label = match format {
                                TextFormat::Txt => "TXT · 纯文本",
                                TextFormat::Markdown => "MD · Markdown",
                                TextFormat::Word => "Word · 可编辑",
                                TextFormat::Pdf => "PDF · 阅读",
                            };
                            let selected = self.text_format == format;
                            let button = egui::Button::new(
                                egui::RichText::new(label).size(12.0).color(if selected {
                                    ACCENT_DARK
                                } else {
                                    MUTED
                                }),
                            )
                            .fill(if selected {
                                SOFT_BLUE
                            } else {
                                egui::Color32::WHITE
                            })
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                if selected { ACCENT } else { BORDER },
                            ))
                            .rounding(7.0);
                            if ui.add(button).clicked() {
                                self.text_format = format;
                            }
                        }
                    });
                });
            }

            if self.mode == Mode::WebText {
                ui.add_space(6.0);
                let previous_scope = self.zhihu_all_answers;
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("获取范围");
                        ui.selectable_value(
                            &mut self.zhihu_all_answers,
                            true,
                            "整个问题 · 包含更多回答",
                        );
                        ui.selectable_value(&mut self.zhihu_all_answers, false, "仅链接中的回答");
                    });
                });
                if previous_scope != self.zhihu_all_answers {
                    self.zhihu_content = None;
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
                } else {
                    caption(ui, "先获取回答数量，再选择范围与格式；获取时不会保存文件。");
                }
            } else if self.mode == Mode::Transcribe {
                ui.add_space(6.0);
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal(|ui| {
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
                                    ui.selectable_value(
                                        &mut self.transcribe_language,
                                        value.into(),
                                        label,
                                    );
                                }
                            });
                    });
                });
            }

            ui.add_space(6.0);
            let enabled = !self.busy
                && if self.mode == Mode::Transcribe {
                    self.media_file.is_some() && self.ffmpeg_ok
                } else {
                    !self.link.trim().is_empty()
                };
            let action = match self.mode {
                Mode::Subtitle => "获取字幕",
                Mode::Video => "获取视频",
                Mode::Douyin => "获取抖音视频",
                Mode::Transcribe => "开始识别",
                Mode::WebText => {
                    if self.zhihu_content.is_some() {
                        "确认导出"
                    } else {
                        "获取内容"
                    }
                }
            };
            if super::reference::wide_button(
                ui,
                if self.busy { "正在处理…" } else { action },
                ui.available_width(),
                enabled,
                super::reference::Symbol::Play,
            ) {
                match self.mode {
                    Mode::Transcribe => self.spawn_transcribe(),
                    Mode::WebText => {
                        if self.zhihu_content.is_some() {
                            self.spawn_export_zhihu();
                        } else {
                            self.spawn_fetch_zhihu();
                        }
                    }
                    _ => self.spawn_fetch_video(),
                }
            }
            // Keep platform errors next to the action, even if the footer is off screen.
            if self.status.contains("抖音") || self.status.starts_with("解析链接失败") {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&self.status).size(12.0).color(INK));
                if self.mode == Mode::Subtitle && external::is_supported(&self.link) {
                    if secondary_button(ui, "切换到抖音下载", !self.busy) {
                        self.mode = Mode::Douyin;
                        self.status = "已切换到抖音下载，点击「获取抖音视频」尝试解析".into();
                    }
                }
            }
            if ui.ctx().screen_rect().height() >= 680.0 {
                ui.add_space(5.0);
                ui.vertical_centered(|ui| {
                    caption(
                        ui,
                        match self.mode {
                            Mode::Subtitle => "AI 字幕需要登录后获取，支持多种文档格式",
                            Mode::Video => "高画质视频需要登录 B站，输出为 MP4",
                            Mode::Douyin => "保存为视频文件；无法解析时可先点右上角「抖音登录」",
                            Mode::Transcribe => "识别完成后保存所选格式与 SRT 字幕，全程在本机处理",
                            Mode::WebText => "先获取 → 选择范围和格式 → 确认导出",
                        },
                    );
                });
            }
            ui.add_space(6.0);
            if self.mode == Mode::Transcribe && !self.ffmpeg_ok {
                ui.label(
                    egui::RichText::new("音视频处理组件未就绪，请检查应用安装。 ")
                        .color(egui::Color32::from_rgb(181, 115, 26)),
                );
            }
        });
    }

    pub(super) fn media_card(&mut self, ui: &mut egui::Ui) {
        if matches!(self.mode, Mode::Transcribe | Mode::WebText) {
            return;
        }
        if self.mode == Mode::Douyin && self.external.is_none() {
            return;
        }
        if let Some(video) = self
            .external
            .clone()
            .filter(|_| matches!(self.mode, Mode::Video | Mode::Douyin))
        {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, "视频已就绪", "抖音 / 小红书");
                ui.label(egui::RichText::new(video.title).size(16.0).strong());
                ui.add_space(6.0);
                if primary_button(ui, "下载视频", !self.busy) {
                    self.spawn_download_external();
                }
                caption(ui, "自动选择最佳画质，保存为 MP4");
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
            ui.label(egui::RichText::new(&title).size(16.0).strong());
            ui.add_space(6.0);
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
                                    ui.selectable_value(
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
                                    ui.selectable_value(
                                        &mut self.selected_stream,
                                        index,
                                        &stream.label,
                                    );
                                }
                            });
                    });
                });
                ui.add_space(6.0);
                if primary_button(ui, "下载视频", !self.busy && !self.streams.is_empty()) {
                    self.spawn_download_video();
                }
                caption(ui, "保存为 MP4；高画质视频需要登录 B站");
                if !self.ffmpeg_ok {
                    caption(ui, "音视频合并组件未就绪，部分画质暂不可下载。");
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
                                    ui.selectable_value(
                                        &mut self.selected_track,
                                        index,
                                        &track.lan_doc,
                                    );
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
        egui::Frame::default()
            .fill(SOFT_BLUE)
            .rounding(10.0)
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.spinner();
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        super::reference::icon(
                            ui.painter(),
                            rect,
                            super::reference::Symbol::Info,
                            status_color(&self.status),
                        );
                    }
                    let message = if self.status.starts_with("已保存:") {
                        self.status
                            .find('（')
                            .map(|i| format!("保存完成 {}", &self.status[i..]))
                            .unwrap_or_else(|| "保存完成，可直接打开文件".into())
                    } else {
                        self.status.clone()
                    };
                    ui.add(egui::Label::new(egui::RichText::new(&message).size(12.0)).truncate())
                        .on_hover_text(&message);
                });
                if let Some(progress) = self.video_progress.filter(|_| self.busy) {
                    ui.add(
                        egui::ProgressBar::new(progress as f32)
                            .desired_width(ui.available_width())
                            .show_percentage(),
                    );
                }
                for path in self.saved_files.clone() {
                    ui.horizontal(|ui| {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        let width = (ui.available_width() - 175.0).max(60.0);
                        ui.add_sized(
                            [width, 24.0],
                            egui::Label::new(egui::RichText::new(filename).size(12.0).color(INK))
                                .truncate(),
                        )
                        .on_hover_text(path.display().to_string());
                        if secondary_button(ui, "打开文件", !self.busy) {
                            if let Err(error) = open_saved_file(&path, false) {
                                self.status = format!("打开失败: {error}");
                            }
                        }
                        if secondary_button(ui, "所在位置", !self.busy) {
                            if let Err(error) = open_saved_file(&path, true) {
                                self.status = format!("打开失败: {error}");
                            }
                        }
                    });
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
        for size in [[1000.0, 760.0], [780.0, 600.0]] {
            for (index, mode) in [
                Mode::Subtitle,
                Mode::Video,
                Mode::Transcribe,
                Mode::WebText,
                Mode::WebText,
                Mode::Transcribe,
                Mode::Douyin,
            ]
            .into_iter()
            .enumerate()
            {
                let ctx = egui::Context::default();
                load_chinese_font(&ctx);
                apply_style(&ctx);
                let mut app = App::new(&ctx);
                app.mode = mode;
                if index == 4 {
                    app.status = "已保存: 界面验证文件.txt".into();
                    app.reveal_result = true;
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
                                text.galley.text() == "打开文件"
                                    && shape.clip_rect.contains(text.pos)
                            } else {
                                false
                            }
                        }),
                        "Saved file action must be visible after automatic scrolling"
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
