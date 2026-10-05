#![cfg(windows)]
#[test]
#[ignore = "需要 Windows WebView2 Runtime，使用本地文章页面验证正文提取"]
fn reader_extracts_rendered_article_without_navigation_or_sidebar_text() {
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
        platform::windows::EventLoopBuilderExtWindows,
        window::{Window, WindowId},
    };
    struct Fixture {
        view: Option<wry::WebView>,
        window: Option<Window>,
        proxy: EventLoopProxy<String>,
        passed: bool,
    }
    impl ApplicationHandler<String> for Fixture {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let window = event_loop
                .create_window(Window::default_attributes().with_visible(false))
                .unwrap();
            // about:blank fixtures have no Douyin hostname. Only the origin gate and shadow visibility change.
            let script = include_str!("../src/douyin_reader.js")
                .replace("!allowed() || ", "")
                .replace("mode:'closed'", "mode:'open'");
            let ready = self.proxy.clone();
            let received = self.proxy.clone();
            let html = format!("<!doctype html><meta charset=utf-8><nav>导航菜单不可导出</nav><article><h1>长文标题</h1><p>{}</p><p>最后一段完整保留</p></article><aside>推荐内容不可导出</aside>", "正文段落。".repeat(300));
            let view = wry::WebViewBuilder::new()
                .with_html(html)
                .with_initialization_script(script)
                .with_on_page_load_handler(move |event, _| {
                    if matches!(event, wry::PageLoadEvent::Finished) {
                        let _ = ready.send_event("ready".into());
                    }
                })
                .with_ipc_handler(move |message| {
                    let _ = received.send_event(message.body().clone());
                })
                .build(&window)
                .unwrap();
            self.view = Some(view);
            self.window = Some(window);
        }
        fn user_event(&mut self, event_loop: &ActiveEventLoop, message: String) {
            if message == "timeout" {
                panic!("Reader did not return fixture text");
            }
            if message == "ready" {
                self.view.as_ref().unwrap().evaluate_script("document.getElementById('shiwen-reader').shadowRoot.getElementById('article').click()").unwrap();
                return;
            }
            let value: serde_json::Value = serde_json::from_str(&message).unwrap();
            let body = value["body"].as_str().unwrap();
            assert!(body.ends_with("最后一段完整保留"));
            assert!(!body.contains("导航菜单"));
            assert!(!body.contains("推荐内容"));
            assert_eq!(value["title"], "长文标题");
            self.passed = true;
            event_loop.exit();
        }
        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let event_loop = EventLoop::<String>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let proxy = event_loop.create_proxy();
    let timeout = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(10));
        let _ = timeout.send_event("timeout".into());
    });
    let mut fixture = Fixture {
        view: None,
        window: None,
        proxy,
        passed: false,
    };
    event_loop.run_app(&mut fixture).unwrap();
    assert!(fixture.passed);
}
