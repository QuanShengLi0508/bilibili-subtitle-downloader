use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn session_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Shiwen")
}

pub fn saved_cookies() -> String {
    std::fs::read(session_directory().join("zhihu-session.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<BTreeMap<String, String>>(&data).ok())
        .map(|cookies| cookie_header(&cookies))
        .unwrap_or_default()
}

pub fn cookie_header(cookies: &BTreeMap<String, String>) -> String {
    cookies
        .iter()
        .filter(|(name, value)| {
            !name.is_empty()
                && !name.contains([';', '=', '\r', '\n'])
                && !value.contains([';', '\r', '\n'])
        })
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn has_saved_session() -> bool {
    saved_cookies().split(';').any(|item| {
        item.trim()
            .strip_prefix("z_c0=")
            .is_some_and(|value| !value.is_empty())
    })
}

pub fn request_identity(cookies: &str) -> (String, String) {
    let fingerprint = cookies
        .split(';')
        .find_map(|part| part.trim().strip_prefix("d_c0="))
        .filter(|value| !value.is_empty())
        .unwrap_or("ZhihuAnonymousFingerprint00000000000000")
        .to_string();
    let header = if cookies.is_empty() {
        format!("d_c0={fingerprint}")
    } else if cookies
        .split(';')
        .any(|part| part.trim().starts_with("d_c0="))
    {
        cookies.to_string()
    } else {
        format!("{cookies}; d_c0={fingerprint}")
    };
    (fingerprint, header)
}

#[cfg(windows)]
pub fn run_window() -> Result<bool> {
    run_site(false)
}

#[cfg(windows)]
pub fn run_douyin_window() -> Result<bool> {
    run_site(true)
}

#[cfg(windows)]
fn run_site(douyin: bool) -> Result<bool> {
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop},
        window::{Window, WindowId},
    };
    struct Login {
        webview: Option<wry::WebView>,
        window: Option<Window>,
        result: Option<Result<bool>>,
        douyin: bool,
    }
    impl ApplicationHandler for Login {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let setup = (|| -> Result<(Window, wry::WebView)> {
                let directory = session_directory().join(if self.douyin {
                    "douyin-webview"
                } else {
                    "zhihu-webview"
                });
                std::fs::create_dir_all(&directory)?;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_title(if self.douyin {
                            "拾文 · 抖音登录（访问或登录后关闭此窗口）"
                        } else {
                            "拾文 · 知乎登录（登录完成后关闭此窗口）"
                        })
                        .with_inner_size(winit::dpi::LogicalSize::new(960.0, 760.0)),
                )?;
                let mut context = wry::WebContext::new(Some(directory));
                let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_url(if self.douyin {
                        "https://www.douyin.com/"
                    } else {
                        "https://www.zhihu.com/signin"
                    })
                    .build(&window)
                    .context("无法打开登录窗口，请安装 Microsoft Edge WebView2 Runtime 后重试")?;
                Ok((window, webview))
            })();
            match setup {
                Ok((window, webview)) => {
                    self.window = Some(window);
                    self.webview = Some(webview);
                }
                Err(error) => {
                    self.result = Some(Err(error));
                    event_loop.exit();
                }
            }
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if matches!(event, WindowEvent::CloseRequested) {
                self.result = Some((|| -> Result<bool> {
                    let webview = self.webview.as_ref().context("登录窗口尚未加载")?;
                    if self.douyin {
                        let mut cookies = Vec::new();
                        for url in ["https://www.douyin.com/", "https://www.iesdouyin.com/"] {
                            cookies.extend(webview.cookies_for_url(url)?);
                        }
                        if cookies.is_empty() {
                            return Ok(false);
                        }
                        let mut lines = std::collections::BTreeSet::new();
                        for cookie in cookies {
                            let domain = cookie.domain().unwrap_or("www.douyin.com");
                            let expiry = cookie
                                .expires_datetime()
                                .map(|time| time.unix_timestamp())
                                .unwrap_or(0);
                            let parts = [
                                domain.to_owned(),
                                if domain.starts_with('.') {
                                    "TRUE"
                                } else {
                                    "FALSE"
                                }
                                .into(),
                                cookie.path().unwrap_or("/").into(),
                                if cookie.secure().unwrap_or(false) {
                                    "TRUE"
                                } else {
                                    "FALSE"
                                }
                                .into(),
                                expiry.to_string(),
                                cookie.name().into(),
                                cookie.value().into(),
                            ];
                            if parts.iter().any(|part| part.contains(['\t', '\r', '\n'])) {
                                continue;
                            }
                            let line = parts.join("\t");
                            lines.insert(if cookie.http_only().unwrap_or(false) {
                                format!("#HttpOnly_{line}")
                            } else {
                                line
                            });
                        }
                        std::fs::write(
                            crate::external::douyin_cookie_path(),
                            format!(
                                "# Netscape HTTP Cookie File\n{}\n",
                                lines.into_iter().collect::<Vec<_>>().join("\n")
                            ),
                        )?;
                        return Ok(true);
                    }
                    let mut cookies = BTreeMap::new();
                    for url in ["https://www.zhihu.com/", "https://zhuanlan.zhihu.com/"] {
                        for cookie in webview.cookies_for_url(url)? {
                            cookies.insert(cookie.name().to_owned(), cookie.value().to_owned());
                        }
                    }
                    if !cookies.get("z_c0").is_some_and(|value| !value.is_empty()) {
                        return Ok(false);
                    }
                    std::fs::write(
                        session_directory().join("zhihu-session.json"),
                        serde_json::to_vec(&cookies)?,
                    )?;
                    Ok(true)
                })());
                event_loop.exit();
            }
        }
    }
    let event_loop = EventLoop::new()?;
    let mut login = Login {
        window: None,
        webview: None,
        result: None,
        douyin,
    };
    event_loop.run_app(&mut login)?;
    login.result.unwrap_or(Ok(false))
}

#[cfg(not(windows))]
pub fn run_window() -> Result<bool> {
    anyhow::bail!("本次知乎登录窗口仅支持 Windows")
}

#[cfg(not(windows))]
pub fn run_douyin_window() -> Result<bool> {
    anyhow::bail!("抖音登录窗口仅支持 Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_and_signature_use_the_same_fingerprint() {
        let (fingerprint, header) = request_identity("z_c0=mock; d_c0=\"test|123\"");
        assert_eq!(fingerprint, "\"test|123\"");
        assert_eq!(header, "z_c0=mock; d_c0=\"test|123\"");
        assert_eq!(
            request_identity("z_c0=mock").1,
            "z_c0=mock; d_c0=ZhihuAnonymousFingerprint00000000000000"
        );
    }
    #[test]
    fn cookie_header_rejects_injected_headers() {
        let cookies = BTreeMap::from([
            ("z_c0".into(), "mock".into()),
            ("bad\nname".into(), "x".into()),
            ("bad".into(), "x; other=y".into()),
        ]);
        assert_eq!(cookie_header(&cookies), "z_c0=mock");
    }
}
