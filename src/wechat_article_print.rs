use anyhow::{Context, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{ICoreWebView2Environment6, ICoreWebView2_7},
    PrintToPdfCompletedHandler,
};
use windows::core::{Interface, PCWSTR};
use wry::WebViewExtWindows;

/// Print the already authenticated, fully displayed article in its existing WebView2.
/// The caller owns `path`; no file path or print settings come from page JavaScript.
pub(super) fn start(
    webview: &wry::WebView,
    path: &Path,
    complete: impl FnOnce(bool) + 'static,
) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    if !path.is_absolute() {
        anyhow::bail!("公众号 PDF 输出路径必须是绝对路径");
    }
    let print_view = webview
        .webview()
        .cast::<ICoreWebView2_7>()
        .context("当前 WebView2 版本不支持原版排版 PDF")?;
    let environment = webview
        .environment()
        .cast::<ICoreWebView2Environment6>()
        .context("当前 WebView2 版本不支持 PDF 打印设置")?;
    let settings =
        unsafe { environment.CreatePrintSettings() }.context("无法创建公众号 PDF 打印设置")?;
    unsafe {
        settings.SetShouldPrintBackgrounds(true)?;
        settings.SetShouldPrintHeaderAndFooter(false)?;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handler = PrintToPdfCompletedHandler::create(Box::new(move |error, success| {
        complete(error.is_ok() && success);
        Ok(())
    }));
    unsafe {
        print_view
            .PrintToPdf(PCWSTR(wide.as_ptr()), &settings, &handler)
            .context("公众号原版排版 PDF 打印未能启动")?;
    }
    Ok(())
}
