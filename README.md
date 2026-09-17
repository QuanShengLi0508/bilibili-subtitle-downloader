# B站字幕下载器

输入B站视频链接，下载视频的 CC 字幕（SRT / TXT）。

## 使用

### 图形界面

双击 `target/release/bili-subtitle-downloader.exe`（或运行 `cargo run --release`）。

1. 粘贴B站视频链接（支持 `www.bilibili.com/video/BV...`、`b23.tv` 短链接、纯 BV 号 / AV 号）。
2. 点击「获取字幕」。
3. 多P视频先选分P，再选字幕语言。
4. 点击「下载 SRT」或「下载 TXT」，文件保存在所选输出目录。

## 登录（重要）

B站限制：**AI 自动生成的字幕必须登录后才能获取**，未登录只能下载 UP 主手动上传的 CC 字幕（现在大多数视频都没有）。

点击「扫码登录」，用B站 App 扫窗口里的二维码即可。登录成功后 Cookie 会保存在 `C:\Users\<用户名>\.bili-subtitle-cookies.json`，下次启动自动生效；点「退出登录」可清除。

### 命令行

```
bili-subtitle-downloader.exe <链接>
```

会把第一条可用字幕保存为 SRT 到 `字幕输出` 文件夹。

## 说明

- AI 字幕（自动生成）和 UP 主上传的 CC 字幕都可以下载；部分内容需要登录后才能获取，匿名请求可能拿不到。
- 基于 [bilibili-API-collect](https://github.com/SocialSisterYi/bilibili-API-collect) 社区公开接口实现，含 WBI 签名。

## 构建

需要 Rust 工具链（本机已安装 GNU 版）：

```
cargo build --release
```
