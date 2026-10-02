# 拾文 Android

手机独立运行，不需要连接电脑。支持 Android 8.0 及以上的 ARM64 手机。

## 功能

- B站字幕提取和视频下载，可在应用内登录 B站。
- 本地音频、视频转写，内置 Whisper base 量化模型，离线识别，不显示模型选择。
- 知乎问题、回答、专栏提取；先显示已获取条数，再选择范围、格式并确认导出。
- 所有文字均可导出 TXT、MD、Word（DOCX）、PDF；字幕和转写同时保留 SRT。
- 保存后直接打开、分享；通过 Android 系统分享菜单选择微信、QQ 等已安装应用，以附件方式发送。
- “导出文件”页面可将文件另存到手机指定位置。

下载和在线内容提取需要网络；平台返回的可见内容及视频画质受账户权限和平台限制影响。转写期间请保持应用在前台，耗时取决于手机性能和音视频时长。分享目标是否接受某种附件格式或大小，由接收应用决定。

## 构建

使用 Flutter stable、Java 17、Android SDK 36、NDK 28.2.13676358、CMake 3.22.1。Windows 上推荐从 ASCII 路径构建，避免本地工具处理中文路径异常。

1. 配置 Android SDK 和 Java。
2. 创建独立发布密钥，将签名参数放入 android/key.properties（storeFile、keyAlias、storePassword、keyPassword）。
3. 在本目录运行：

```text
flutter pub get
flutter test
flutter build apk --release --target-platform android-arm64
```

签名密钥和密码文件已忽略版本控制；请备份本机密钥，后续更新必须使用同一密钥。

## 第三方组件

vendor/whisper_flutter_new 保留上游 GPLv3 许可证与源码，并增加 Android 16 KB 页面兼容构建参数。Android 应用源码遵循随附 GPLv3 许可证。Noto Sans SC 字体遵循 OFL；Whisper 模型、FFmpeg 及其他 Flutter 组件保留各自许可。桌面项目的原有许可证见仓库根目录。

