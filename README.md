# B站字幕/视频下载器

输入B站视频链接，下载视频的 CC 字幕（TXT / SRT）或下载视频（MP4，可选清晰度）。

## 使用

### 图形界面

双击 `target/release/bili-subtitle-downloader.exe`（或运行 `cargo run --release`）。

1. 粘贴B站视频链接（支持 `www.bilibili.com/video/BV...`、`b23.tv` 短链接、纯 BV 号 / AV 号）。
2. 在「下载字幕」和「下载视频」之间切换模式。
3. 多P视频先选分P。
4. 字幕模式：选择字幕语言，点击「下载 TXT」或「下载 SRT」。
5. 视频模式：选择清晰度，点击「下载视频」；DASH 视频会下载画面和音频，并用 ffmpeg 合并为 MP4。

文件默认保存在 `字幕输出` 文件夹，也可以在底部点击「更改」选择其他目录。

## 登录（重要）

B站限制：

- **AI 自动生成的字幕必须登录后才能获取**，未登录只能下载 UP 主手动上传的 CC 字幕。
- **4K、1080P 高码率等高画质通常也需要登录**，未登录时可能只能看到 720P / 480P / 360P。

点击「扫码登录」，用B站 App 扫窗口里的二维码即可。登录成功后 Cookie 会保存在 `C:\Users\<用户名>\.bili-subtitle-cookies.json`，下次启动自动生效；点「退出登录」可清除。

### 视频下载依赖

高画质B站视频通常是 DASH 格式，需要 ffmpeg 合并音视频：

```powershell
winget install Gyan.FFmpeg
```

安装后重新启动本程序。程序启动时会自动检测 ffmpeg；未检测到时会在视频模式里给出提示。

### 命令行

```text
bili-subtitle-downloader.exe <链接>              # 下载第一条字幕为 TXT
bili-subtitle-downloader.exe --srt <链接>        # 下载第一条字幕为 SRT
bili-subtitle-downloader.exe --streams <链接>    # 查看可用画质
bili-subtitle-downloader.exe --video <链接> [qn] # 下载视频，默认最高画质
```

例如查看画质后下载 1080P（清晰度 ID 为 80）：

```text
bili-subtitle-downloader.exe --streams BV1xxxxxxxx
bili-subtitle-downloader.exe --video BV1xxxxxxxx 80
```

## 说明

- AI 字幕（自动生成）和 UP 主上传的 CC 字幕都可以下载；部分内容需要登录后才能获取。
- 视频和字幕仅保存到本机，本程序不会上传任何内容。
- 基于 [bilibili-API-collect](https://github.com/SocialSisterYi/bilibili-API-collect) 社区公开接口实现，含 WBI 签名。

## 音频/视频转文字

在图形界面选择「音频/视频转文字」模式，选择本地音频或视频文件后点击「开始识别」。程序会用 ffmpeg 提取 16kHz 单声道音频，然后用内置的 whisper-cli 生成 TXT 和 SRT。

- 默认模型：`whisper/ggml-base-q5_1.bin`，也可以点击「选择模型」使用其他 GGML 格式 Whisper 模型。
- 识别默认使用自动语言检测，也可以手动选择中文或 English。
- 音频和文件只在本地处理，不会上传到服务器。
## 构建

需要 Rust 工具链（本机已安装 GNU 版）：

```powershell
cargo build --release
```
