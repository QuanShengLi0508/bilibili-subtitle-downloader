# 拾文

> **下载地址：** [Windows 安装包 / 免安装 ZIP（最新版）](https://github.com/QuanShengLi0508/bilibili-subtitle-downloader/releases/latest)

支持B站、抖音、小红书链接。B站可下载字幕或视频；抖音和小红书可下载视频。

## 使用

### 图形界面

双击项目根目录的 `拾文.exe`（或运行 `cargo run --release`）。图形版启动时不会弹出终端窗口。

1. 粘贴B站视频链接（支持 `www.bilibili.com/video/BV...`、`b23.tv` 短链接、纯 BV 号 / AV 号）。
   在链接输入框按回车也可以获取内容。
2. 在「下载字幕」和「下载视频」之间切换模式。
3. 多P视频先选分P。
4. 字幕模式：选择字幕语言，点击「下载 TXT」或「下载 SRT」。
5. 视频模式：选择清晰度，点击「下载视频」；DASH 视频会下载画面和音频，并用 ffmpeg 合并为 MP4。

文字导出可选择 TXT、Markdown、Word（.docx）或 PDF，适用于字幕、知乎文字和音视频转写；字幕与转写仍支持 SRT。Word 和 PDF 保留文字段落，并添加文档标题，PDF 自动换行分页。

文件默认保存在 `字幕输出` 文件夹，也可以在底部点击「更改」选择其他目录。保存后直接点击结果旁的「打开文件」或「打开所在位置」，无需手动寻找文件。导出为其他格式时也保留原始 TXT。
下载中会显示进度；失败的下载不会留下不完整的成品文件。

## 登录（重要）

B站限制：

- **AI 自动生成的字幕必须登录后才能获取**，未登录只能下载 UP 主手动上传的 CC 字幕。
- **4K、1080P 高码率等高画质通常也需要登录**，未登录时可能只能看到 720P / 480P / 360P。

点击「扫码登录」，用B站 App 扫窗口里的二维码即可。登录成功后 Cookie 会保存在 `C:\Users\<用户名>\.bili-subtitle-cookies.json`，下次启动自动生效；点「退出登录」可清除。
扫码等待期间可以点击「取消登录」。

### 视频下载依赖

高画质B站视频通常是 DASH 格式，需要 ffmpeg 合并音视频：

```powershell
winget install Gyan.FFmpeg
```

安装后重新启动本程序。程序启动时会自动检测 ffmpeg；未检测到时会在视频模式里给出提示。

### 命令行

```text
bili-subtitle-cli.exe <链接>              # 下载第一条字幕为 TXT
bili-subtitle-cli.exe --srt <链接>        # 下载第一条字幕为 SRT
bili-subtitle-cli.exe --streams <链接>    # 查看可用画质
bili-subtitle-cli.exe --video <链接> [qn] # 下载视频，默认最高画质
```

例如查看画质后下载 1080P（清晰度 ID 为 80）：

```text
bili-subtitle-cli.exe --streams BV1xxxxxxxx
bili-subtitle-cli.exe --video BV1xxxxxxxx 80
```

## 说明

- AI 字幕（自动生成）和 UP 主上传的 CC 字幕都可以下载；部分内容需要登录后才能获取。
- 视频和字幕仅保存到本机，本程序不会上传任何内容。
- 基于 [bilibili-API-collect](https://github.com/SocialSisterYi/bilibili-API-collect) 社区公开接口实现，含 WBI 签名。

## 抖音 / 小红书

把视频链接粘贴到顶部，切换到「下载视频」后点「获取视频」。程序会自动用内置的 yt-dlp 解析链接，然后下载最佳画质并输出 MP4。

支持 `douyin.com`、`iesdouyin.com`、`xiaohongshu.com`、`xhslink.com` 链接。

## 知乎 / 网页文本

选择「知乎文本」模式，粘贴知乎问题、回答或专栏链接，先点击「获取内容」查看回答数量，再填写导出范围、选择格式，点击「确认导出」。获取时仅缓存在内存中，确认导出后才保存文件。回答链接默认导出所属问题的全部回答，包含页面下方的「更多回答」；也可选择「仅链接中的回答」。程序会逐页获取回答并去重，支持指定起止序号，保存后显示回答总数及导出范围。批量文件名带回答范围，避免覆盖单条回答文件。专栏链接只导出文章正文。



## 音频/视频转文字

在图形界面选择「音频/视频转文字」模式，选择本地音频或视频文件后点击「开始识别」。程序会用 ffmpeg 提取 16kHz 单声道音频，然后用内置的 whisper-cli 生成 TXT 和 SRT。

- 程序自动使用内置识别模型，无需选择模型文件。
- 识别默认使用自动语言检测，也可以手动选择中文或 English。
- 音频和文件只在本地处理，不会上传到服务器。
## 构建

需要 Rust 工具链。在 PowerShell 中执行：

```powershell
.\build-release.ps1
```

脚本会更新根目录的图形版、命令行版和 `release-package` 目录；若要生成安装包，再用 Inno Setup 编译 `installer/installer.iss`。

### 第三方组件

- 抖音 / 小红书下载使用 [yt-dlp](https://github.com/yt-dlp/yt-dlp)。安装包里已包含 `tools/yt-dlp.exe`；如果自己编译，请从 yt-dlp 的 Release 页下载该文件并放到 `tools/yt-dlp.exe`。
- 本地语音识别使用 [whisper.cpp](https://github.com/ggml-org/whisper.cpp)。安装包里已包含 `whisper-cli.exe` 和基础模型；如果自己编译，也需要把对应文件放到 `whisper/` 文件夹。

## 安卓手机版

安卓源码位于 `mobile/`，手机独立运行，支持 B站字幕与视频、知乎文字及本地音视频转写，内置识别模型。文字支持 TXT、MD、DOCX、PDF，保存后可直接打开或通过系统菜单分享给微信、QQ等应用。

Windows 开发环境可运行 `build-android.ps1` 生成正式签名的 ARM64 APK。签名配置和详细构建说明见 [手机版说明](mobile/README.md)。发布密钥与密码文件不进入版本控制。

## 安装包与发帖图片

[拾文发布下载](https://github.com/QuanShengLi0508/bilibili-subtitle-downloader/releases/tag/shiwen-v1.4.4)提供 Windows 1.4.4 安装版、Android 1.0.0 测试版和发帖截图压缩包。

[发帖截图](发帖截图/截图说明.md)包含 5 张电脑图片和 6 张手机图片，按平台分目录保存。安卓本次已验证知乎获取、PDF 导出、更新后文件保留和系统附件分享；音视频转写及 B站完整流程仍需真机测试，详见[测试记录](mobile/测试记录.md)。
