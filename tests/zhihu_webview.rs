#![cfg(windows)]

#[test]
#[ignore = "需要 Windows WebView2 Runtime，验证原生登录窗口的会话读取能力"]
fn native_webview_reads_http_only_session_cookies() {
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
        platform::windows::EventLoopBuilderExtWindows,
        window::{Window, WindowId},
    };
    struct Smoke {
        profile: std::path::PathBuf,
        webview: Option<wry::WebView>,
        window: Option<Window>,
        proxy: EventLoopProxy<bool>,
        passed: bool,
    }
    impl ApplicationHandler<bool> for Smoke {
        fn resumed(&mut self, _: &ActiveEventLoop) {}
        fn user_event(&mut self, event_loop: &ActiveEventLoop, read: bool) {
            if self.window.is_none() {
                let window = event_loop
                    .create_window(Window::default_attributes().with_visible(false))
                    .unwrap();
                let mut context = wry::WebContext::new(Some(self.profile.clone()));
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_url("about:blank")
                    .with_focused(false)
                    .build(&window)
                    .unwrap();
                self.webview = Some(webview);
                self.window = Some(window);
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    proxy.send_event(false).unwrap();
                });
                return;
            }
            let webview = self.webview.as_ref().unwrap();
            if !read {
                let cookie = wry::cookie::Cookie::build(("z_c0", "shiwen-test-session"))
                    .domain("www.zhihu.com")
                    .path("/")
                    .max_age(wry::cookie::time::Duration::days(1))
                    .secure(true)
                    .http_only(true)
                    .build();
                webview.set_cookie(&cookie).unwrap();
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    proxy.send_event(true).unwrap();
                });
                return;
            }
            let cookies = webview.cookies_for_url("https://www.zhihu.com/").unwrap();
            assert!(
                cookies.iter().any(|c| c.name() == "z_c0"
                    && c.value() == "shiwen-test-session"
                    && c.http_only() == Some(true)),
                "Synthetic session missing: {cookies:?}"
            );
            assert!(webview
                .cookies_for_url("https://www.bilibili.com/")
                .unwrap()
                .is_empty());
            self.passed = true;
            event_loop.exit();
        }
        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let temp = tempfile::tempdir().unwrap();
    let event_loop = EventLoop::<bool>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let proxy = event_loop.create_proxy();
    proxy.send_event(false).unwrap();
    let mut smoke = Smoke {
        profile: temp.path().join("webview"),
        webview: None,
        window: None,
        proxy,
        passed: false,
    };
    event_loop.run_app(&mut smoke).unwrap();
    assert!(smoke.passed);
    smoke.webview.take();
    smoke.window.take();
}
