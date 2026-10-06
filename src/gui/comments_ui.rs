use super::*;

impl App {
    pub(super) fn render_comments(&mut self, ctx: &egui::Context) {
        if !self.comment_window_open {
            return;
        }
        let Some(comments) = self.comments.clone() else {
            return;
        };
        let mut open = self.comment_window_open;
        let mut export = false;
        let width = (ctx.screen_rect().width() - 80.0).clamp(300.0, 620.0);
        let response = egui::Window::new("选择评论后导出")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .collapsible(false)
            .fixed_size([width, (ctx.screen_rect().height() - 100.0).max(300.0)])
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.label(egui::RichText::new(&comments.title).strong());
                ui.label(format!(
                    "已检测 {} 条（含展开回复） · 已选择 {} 条",
                    comments.items.len(),
                    self.comment_selected.iter().filter(|value| **value).count()
                ));
                if !comments.total_label.is_empty() {
                    ui.label(format!("页面数量：{}", comments.total_label));
                }
                ui.label(
                    egui::RichText::new(if comments.items.iter().any(|c| c.likes.contains('+')) {
                        "仅包含已加载评论；“10+”等按显示值排序，确认后才保存。"
                    } else {
                        "仅包含页面已加载评论；默认不勾选，确认后才保存。"
                    })
                    .size(12.0)
                    .color(egui::Color32::GRAY),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.comment_filter)
                                .hint_text("搜索作者或评论文字")
                                .desired_width(260.0),
                        )
                        .changed()
                    {
                        self.comment_page = 0;
                    }
                    if ui.button("取消全选").clicked() {
                        self.comment_selected.fill(false);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("排序");
                    let before = self.comment_sort;
                    let (mut basis, mut reverse) = self.comment_sort.options();
                    egui::ComboBox::from_id_salt("comment_sort_basis")
                        .width(120.0)
                        .selected_text(["原始顺序", "点赞数", "发布时间"][basis])
                        .show_ui(ui, |ui| {
                            for (index, label) in
                                ["原始顺序", "点赞数", "发布时间"].iter().enumerate()
                            {
                                ui.selectable_value(&mut basis, index, *label);
                            }
                        });
                    ui.selectable_value(&mut reverse, false, "正序 ↑");
                    ui.selectable_value(&mut reverse, true, "倒序 ↓");
                    self.comment_sort = crate::comments::Sort::from_options(basis, reverse);
                    ui.label(self.comment_sort.label());
                    if before != self.comment_sort {
                        self.comment_page = 0;
                    }
                });
                let filtered: Vec<_> = comments
                    .ordered(self.comment_sort)
                    .into_iter()
                    .filter(|index| {
                        let item = &comments.items[*index];
                        self.comment_filter.trim().is_empty()
                            || format!("{} {} {}", item.author, item.body, item.reply_to)
                                .to_lowercase()
                                .contains(&self.comment_filter.trim().to_lowercase())
                    })
                    .collect();
                let page_count = filtered.len().div_ceil(6).max(1);
                self.comment_page = self.comment_page.min(page_count - 1);
                ui.horizontal(|ui| {
                    if ui.button("全选搜索结果").clicked() {
                        for index in &filtered {
                            self.comment_selected[*index] = true;
                        }
                    }
                    if ui
                        .add_enabled(self.comment_page > 0, egui::Button::new("上一页"))
                        .clicked()
                    {
                        self.comment_page -= 1;
                    }
                    ui.label(format!(
                        "{} / {} 页 · 搜索结果 {} 条",
                        self.comment_page + 1,
                        page_count,
                        filtered.len()
                    ));
                    if ui
                        .add_enabled(
                            self.comment_page + 1 < page_count,
                            egui::Button::new("下一页"),
                        )
                        .clicked()
                    {
                        self.comment_page += 1;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("comment_selection_list")
                    .max_height((ctx.screen_rect().height() - 460.0).clamp(100.0, 300.0))
                    .show(ui, |ui| {
                        if filtered.is_empty() {
                            ui.label("没有匹配的评论");
                        }
                        for index in filtered.iter().skip(self.comment_page * 6).take(6) {
                            let item = &comments.items[*index];
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.comment_selected[*index],
                                    format!(
                                        "{} · {}",
                                        index + 1,
                                        if item.author.is_empty() {
                                            "未显示作者"
                                        } else {
                                            &item.author
                                        }
                                    ),
                                );
                                if !item.likes.is_empty() {
                                    ui.label(format!("赞 {}", item.likes));
                                }
                                if !item.time.is_empty() {
                                    ui.label(&item.time);
                                }
                            });
                            if !item.thread.is_empty() {
                                ui.label(format!("所属回答：{}", item.thread));
                            }
                            if !item.reply_to.is_empty() {
                                ui.label(format!("回复：{}", item.reply_to));
                            }
                            ui.add(egui::Label::new(&item.body).wrap());
                            ui.add_space(6.0);
                            ui.separator();
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("导出格式");
                    for format in TextFormat::ALL {
                        ui.selectable_value(&mut self.text_format, format, format.label());
                    }
                });
                let count = self.comment_selected.iter().filter(|v| **v).count();
                if ui
                    .add_enabled(
                        !self.busy && count > 0,
                        egui::Button::new(format!("确认导出所选 {count} 条评论")),
                    )
                    .clicked()
                {
                    export = true;
                }
            });
        #[cfg(test)]
        if let Some(response) = response {
            let fits = ctx.screen_rect().contains_rect(response.response.rect);
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("comments_fit"), fits));
        }
        #[cfg(not(test))]
        let _ = response;
        self.comment_window_open = open;
        if export {
            self.export_comments();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_window_keeps_export_controls_visible_at_both_sizes() {
        for size in [egui::vec2(1000.0, 760.0), egui::vec2(780.0, 600.0)] {
            let ctx = egui::Context::default();
            load_chinese_font(&ctx);
            apply_style(&ctx);
            let mut app = App::new(&ctx);
            let source = "https://www.douyin.com/note/123";
            let data = serde_json::json!({"source":source,"title":"评论选择与排序","total_label":"3234","comments":(0..12).map(|i|serde_json::json!({"id":i.to_string(),"author":format!("作者{i}"),"body":"这是一条较长评论，包含完整正文，用于检查小窗口中的选择与导出操作。".repeat(8),"likes":"1.2万","posted_at":i*100,"time":"1周前"})).collect::<Vec<_>>()});
            app.comments = Some(Arc::new(
                crate::comments::Comments::from_message(&data.to_string(), source).unwrap(),
            ));
            app.comment_selected = vec![true; 12];
            app.comment_sort = crate::comments::Sort::Likes;
            app.comment_window_open = true;
            let mut output = None;
            for frame in 0..3 {
                output = Some(ctx.run(
                    egui::RawInput {
                        time: Some(frame as f64),
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| app.render_comments(ctx),
                ));
            }
            assert!(
                ctx.data(|data| data.get_temp::<bool>(egui::Id::new("comments_fit")))
                    .unwrap_or(false),
                "评论选择窗口超出屏幕 {size:?}"
            );
            let output = output.unwrap();
            assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::epaint::Shape::Text(t) if t.galley.text().contains("确认导出所选") && s.clip_rect.contains_rect(egui::Rect::from_min_size(t.pos,t.galley.size())))),"导出按钮被裁切 {size:?}");
            assert_eq!(app.comment_selected.iter().filter(|v| **v).count(), 12);
        }
    }
}
