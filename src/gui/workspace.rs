use super::*;

impl Mode {
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Subtitle | Self::Video | Self::BiliText => "bilibili",
            Self::Douyin => "douyin",
            Self::Xhs => "xhs",
            Self::Youtube => "youtube",
            Self::WebText => "zhihu",
            Self::WechatArticle => "wechat_article",
            Self::WechatChannels => "wechat_channels",
            Self::Twitter => "twitter",
            Self::Transcribe => "transcribe",
        }
    }
    pub(super) fn from_key(key: &str) -> Self {
        match key {
            "douyin" => Self::Douyin,
            "xhs" => Self::Xhs,
            "youtube" => Self::Youtube,
            "zhihu" => Self::WebText,
            "wechat_article" => Self::WechatArticle,
            "wechat_channels" => Self::WechatChannels,
            "twitter" => Self::Twitter,
            "transcribe" => Self::Transcribe,
            _ => Self::Subtitle,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Subtitle | Self::Video | Self::BiliText => "哔哩哔哩",
            Self::Douyin => "抖音",
            Self::Xhs => "小红书",
            Self::Youtube => "YouTube",
            Self::WebText => "知乎",
            Self::WechatArticle => "微信公众号",
            Self::WechatChannels => "微信视频号",
            Self::Twitter => "X / 推特",
            Self::Transcribe => "本地转写",
        }
    }
}

pub(super) fn default_output_directory() -> PathBuf {
    let legacy = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("字幕输出")))
        .unwrap_or_else(output_dir);
    if legacy.is_dir() {
        return legacy;
    }
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join("Documents"))
        .filter(|p| p.is_dir())
        .map(|p| p.join("拾文"))
        .unwrap_or(legacy)
}

fn detect(input: &str) -> Option<Mode> {
    if crate::twitter::link(input).is_some() {
        return Some(Mode::Twitter);
    }
    if crate::wechat_article::article_url(input).is_some() {
        return Some(Mode::WechatArticle);
    }
    if crate::wechat_channels::link(input).is_some() {
        return Some(Mode::WechatChannels);
    }
    if crate::youtube::link(input).is_some() {
        return Some(Mode::Youtube);
    }
    if crate::external::is_xhs(input) {
        return Some(Mode::Xhs);
    }
    if crate::external::is_douyin(input) {
        return Some(Mode::Douyin);
    }
    if let Some(url) = crate::douyin_article::bili_url(input) {
        // A b23 link does not reveal whether the target is a video or an article.
        return Some(
            if reqwest::Url::parse(&url).ok()?.host_str() == Some("b23.tv") {
                Mode::Subtitle
            } else {
                Mode::BiliText
            },
        );
    }
    if let Some(url) = crate::comments::link(input) {
        match crate::comments::platform(&url) {
            Some("bili") => return Some(Mode::Subtitle),
            Some("zhihu") => return Some(Mode::WebText),
            _ => {}
        }
    }
    None
}

impl App {
    pub(super) fn clear_content(&mut self) {
        self.clear_wechat();
        self.twitter_post = None;
        self.twitter_preview_open = false;
        self.comments = None;
        self.comment_selected.clear();
        self.comment_window_open = false;
        self.youtube = None;
        self.external = None;
        self.video = None;
        self.douyin_article = None;
        self.douyin_preview_open = false;
        self.zhihu_content = None;
        self.tracks.clear();
        self.streams.clear();
        self.video_progress = None;
    }

    pub(super) fn switch_workspace(&mut self, mode: Mode) {
        if self.busy {
            return;
        }
        self.link_drafts
            .insert(self.mode.key().into(), self.link.clone());
        self.clear_content();
        self.mode = mode;
        self.link = self
            .link_drafts
            .get(mode.key())
            .cloned()
            .unwrap_or_default();
        self.status = if mode == Mode::Transcribe {
            "选择音频或视频文件，然后开始识别".into()
        } else {
            format!("{} · 粘贴内容链接，获取后预览并保存", mode.label())
        };
    }

    pub(super) fn link_changed(&mut self) -> bool {
        self.clear_content();
        let detected = detect(&self.link);
        let changed = detected.is_some_and(|mode| {
            mode.key() != self.mode.key()
                || mode == Mode::BiliText && self.mode != Mode::BiliText
                || mode == Mode::Subtitle
                    && self.mode == Mode::BiliText
                    && self.link.contains("bilibili.com/video/")
        });
        if let Some(mode) = detected.filter(|_| changed) {
            self.mode = mode;
            self.status = format!("已识别为 {}，选择内容类型后获取", mode.label());
        }
        self.link_drafts
            .insert(self.mode.key().into(), self.link.clone());
        changed
    }

    pub(super) fn save_preferences(&mut self, force: bool) {
        if !force && self.preferences_checked.elapsed() < std::time::Duration::from_secs(2) {
            return;
        }
        self.preferences_checked = std::time::Instant::now();
        let mut next = self.preferences.clone();
        next.platform = self.mode.key().into();
        next.text_format = self.text_format;
        next.output_dir = self.output_dir.clone();
        next.transcribe_language = self.transcribe_language.clone();
        next.speaker_diarization = self.speaker_diarization;
        next.speaker_count = self.speaker_count;
        next.recent_files = self.saved_files.clone();
        if next == self.preferences {
            return;
        }
        #[cfg(not(test))]
        if let Err(error) = next.save() {
            self.preferences_error = Some(format!("设置暂未保存：{error}。本次使用不受影响。"));
            return;
        }
        self.preferences_error = None;
        self.preferences = next;
    }

    pub(super) fn workspace_windows(&mut self, ctx: &egui::Context) {
        if self.status_details_open {
            egui::Window::new("处理详情")
                .open(&mut self.status_details_open)
                .default_width(560.0)
                .max_width((ctx.screen_rect().width() - 80.0).max(300.0))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            ui.label(&self.status);
                        });
                    if ui.button("复制详情").clicked() {
                        ctx.copy_text(self.status.clone());
                    }
                });
        }
        if self.history_open {
            let mut open = self.history_open;
            egui::Window::new("最近保存")
                .open(&mut open)
                .default_width(560.0)
                .max_width((ctx.screen_rect().width() - 80.0).max(300.0))
                .show(ctx, |ui| {
                    ui.label("最近 10 项；文件和图文文件夹都可直接打开");
                    egui::ScrollArea::vertical()
                        .max_height((ctx.screen_rect().height() - 180.0).max(150.0))
                        .show(ui, |ui| {
                            for path in self.saved_files.clone() {
                                ui.horizontal(|ui| {
                                    let name =
                                        path.file_name().unwrap_or_default().to_string_lossy();
                                    ui.add_sized(
                                        [(ui.available_width() - 145.0).max(130.0), 30.0],
                                        egui::Label::new(name).truncate(),
                                    )
                                    .on_hover_text(path.display().to_string());
                                    for (text, reveal) in [("打开", false), ("位置", true)] {
                                        if ui.button(text).clicked() {
                                            if let Err(error) = open_saved_file(&path, reveal) {
                                                self.status = format!("打开失败：{error}");
                                            }
                                        }
                                    }
                                });
                            }
                        });
                });
            self.history_open = open;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pasted_links_route_to_correct_platform_and_keep_separate_drafts() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.link = "https://www.bilibili.com/video/BV1tG4y147F9".into();
        app.link_changed();
        app.link = "https://x.com/NASA/status/2018361967644127646".into();
        assert!(app.link_changed());
        assert!(app.mode == Mode::Twitter);
        app.switch_workspace(Mode::Subtitle);
        assert!(app.link.contains("bilibili.com"));
        app.switch_workspace(Mode::Twitter);
        assert!(app.link.contains("x.com"));
        app.busy = true;
        app.switch_workspace(Mode::WebText);
        assert!(app.mode == Mode::Twitter);
        assert!(detect("https://x.com.evil.test/NASA/status/123").is_none());
        assert!(detect("https://mp.weixin.qq.com/s/Example").unwrap() == Mode::WechatArticle);
    }
    #[test]
    fn old_results_are_cleared_and_bili_video_mode_is_preserved() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.mode = Mode::Video;
        app.tracks.push(SubTrack {
            lan: "zh".into(),
            lan_doc: "字幕".into(),
            url: "old".into(),
        });
        app.link = "https://www.bilibili.com/video/BV1tG4y147F9".into();
        assert!(!app.link_changed());
        assert!(app.mode == Mode::Video);
        assert!(app.tracks.is_empty());
        app.link = "https://www.bilibili.com/opus/123456".into();
        assert!(app.link_changed());
        assert!(app.mode == Mode::BiliText);
        app.link = "https://www.bilibili.com/video/BV1tG4y147F9".into();
        assert!(app.link_changed());
        assert!(app.mode == Mode::Subtitle);
        assert!(detect("https://b23.tv/example").unwrap() == Mode::Subtitle);
        assert!(detect("https://www.bilibili.com/opus/123456").unwrap() == Mode::BiliText);
    }
}
