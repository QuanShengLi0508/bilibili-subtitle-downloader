import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import 'package:open_filex/open_filex.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';
import 'package:ffmpeg_kit_flutter_new_min/ffmpeg_kit.dart';
import 'package:ffmpeg_kit_flutter_new_min/return_code.dart';
import 'package:whisper_flutter_new/whisper_flutter_new.dart';
import 'models.dart';
import 'services/content_service.dart';
import 'services/exports.dart';

const blue = Color(0xff2479ff);
const ink = Color(0xff15294c);
void main() {
  WidgetsFlutterBinding.ensureInitialized();
  LicenseRegistry.addLicense(() async* {
    yield LicenseEntryWithLineBreaks([
      'Noto Sans SC',
    ], await rootBundle.loadString('assets/NotoSansSC-OFL.txt'));
  });
  runApp(const ShiwenApp());
}

class ShiwenApp extends StatelessWidget {
  const ShiwenApp({super.key});
  @override
  Widget build(BuildContext context) => MaterialApp(
    title: '拾文',
    debugShowCheckedModeBanner: false,
    theme: ThemeData(
      colorScheme: ColorScheme.fromSeed(seedColor: blue),
      scaffoldBackgroundColor: const Color(0xfff0f6ff),
      useMaterial3: true,
      textTheme: ThemeData().textTheme.apply(bodyColor: ink, displayColor: ink),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: const Color(0xfff6f9ff),
        contentPadding: const EdgeInsets.symmetric(
          horizontal: 12,
          vertical: 12,
        ),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(12),
          borderSide: const BorderSide(color: Color(0xffd9e6fc)),
        ),
      ),
    ),
    home: const HomePage(),
  );
}

class HomePage extends StatefulWidget {
  const HomePage({super.key});
  @override
  State<HomePage> createState() => _HomeState();
}

class _HomeState extends State<HomePage> {
  final link = TextEditingController();
  final first = TextEditingController(text: '1');
  final last = TextEditingController();
  final service = ContentService();
  final recent = <File>[];
  TextPreview? preview;
  BiliVideo? video;
  List<Map<String, dynamic>> tracks = [];
  List<Map<String, dynamic>> streams = [];
  int mode = 0, page = 0, track = 0, quality = 0;
  bool allAnswers = true, busy = false;
  String status = '粘贴链接，开始获取内容';
  File? media;
  ExportFormat format = ExportFormat.txt;
  double? progress;
  @override
  void initState() {
    super.initState();
    _restore();
  }

  @override
  void dispose() {
    link.dispose();
    first.dispose();
    last.dispose();
    super.dispose();
  }

  Future<void> _restore() async {
    final preferences = await SharedPreferences.getInstance();
    service.biliCookies = preferences.getString('biliCookies') ?? '';
    final dir = await outputDirectory();
    final files = await dir
        .list()
        .where((entry) => entry is File && !entry.path.endsWith('.part'))
        .cast<File>()
        .toList();
    files.sort((a, b) => b.lastModifiedSync().compareTo(a.lastModifiedSync()));
    if (mounted) setState(() => recent.addAll(files.take(3)));
  }

  void invalidate() => setState(() {
    preview = null;
    video = null;
    tracks = [];
    streams = [];
    page = track = quality = 0;
    progress = null;
    status = '内容已更改，请重新获取';
  });
  Future<void> runTask(Future<void> Function() task, String message) async {
    if (busy) return;
    setState(() {
      busy = true;
      status = message;
      progress = null;
    });
    try {
      await task();
    } catch (error) {
      if (mounted) {
        setState(() {
          status = '处理失败：$error';
        });
      }
    } finally {
      if (mounted) {
        setState(() {
          busy = false;
        });
      }
    }
  }

  Future<void> fetch() => runTask(() async {
    if (mode == 3) {
      final value = await service.zhihu(
        link.text,
        allAnswers,
        onCount: (count) {
          if (mounted) setState(() => status = '已获取 $count 条回答，正在继续…');
        },
      );
      if (!mounted) return;
      setState(() {
        preview = value;
        first.text = '1';
        last.text = '${value.entries.length}';
        status =
            '已获取 ${value.entries.length} 条${value.isQuestion ? '回答' : '内容'}，选择范围后确认导出';
      });
    } else {
      final value = await service.bili(link.text);
      if (!mounted) return;
      setState(() {
        video = value;
        page = 0;
      });
      await fetchBiliOptions();
    }
  }, '正在获取内容…');
  Future<void> fetchBiliOptions() async {
    if (video == null) return;
    if (mode == 0) {
      final values = await service.subtitles(video!, page);
      if (mounted) {
        setState(() {
          tracks = values;
          track = 0;
          status = values.isEmpty
              ? '未找到字幕，可尝试登录 B站或使用音视频转文字'
              : '已获取 ${values.length} 条字幕，请选择后导出';
        });
      }
    } else {
      final values = await service.streams(video!, page);
      if (mounted) {
        setState(() {
          streams = values;
          quality = 0;
          status = '已获取 ${values.length} 个画质，请选择后下载';
        });
      }
    }
  }

  Future<void> save() => runTask(() async {
    TextPreview? value = preview;
    if (mode == 0 && video != null && tracks.isNotEmpty) {
      value = await service.subtitleText(video!.title, tracks[track]);
    }
    if (value == null) throw StateError('请先获取内容');
    final from = value.isQuestion
        ? (first.text.trim().isEmpty ? 1 : int.tryParse(first.text))
        : 1;
    final to = value.isQuestion
        ? (last.text.trim().isEmpty
              ? value.entries.length
              : int.tryParse(last.text))
        : value.entries.length;
    if (from == null || to == null) throw ArgumentError('请输入正确的起止序号');
    final file = await exportPreview(value, from, to, format);
    final files = [file];
    if (value.srt.isNotEmpty) {
      final srt = File(p.setExtension(file.path, '.srt'));
      await srt.writeAsString(value.srt);
      files.add(srt);
    }
    await remember(files);
    if (mounted) {
      setState(
        () => status =
            '保存完成${value!.isQuestion ? '：导出第 $from–$to 条回答' : ''}，可直接打开或分享',
      );
    }
  }, '正在导出…');
  Future<void> remember(List<File> files) async {
    if (mounted) {
      setState(() {
        recent.insertAll(0, files);
        if (recent.length > 5) recent.removeRange(5, recent.length);
      });
    }
  }

  Future<void> download() => runTask(() async {
    if (video == null || streams.isEmpty) throw StateError('请先获取视频');
    final file = await service.downloadVideo(video!.title, streams[quality], (
      value,
    ) {
      if (mounted) setState(() => progress = value);
    });
    await remember([file]);
    if (mounted) setState(() => status = '视频保存完成，可直接打开或分享');
  }, '正在下载视频…');
  Future<void> transcribe() => runTask(() async {
    final input = media;
    if (input == null) throw StateError('请先选择音视频文件');
    final dir = await getTemporaryDirectory();
    final wav = File(
      p.join(dir.path, 'shiwen-${DateTime.now().millisecondsSinceEpoch}.wav'),
    );
    try {
      final session = await FFmpegKit.executeWithArguments([
        '-y',
        '-i',
        input.path,
        '-vn',
        '-ac',
        '1',
        '-ar',
        '16000',
        '-c:a',
        'pcm_s16le',
        wav.path,
      ]);
      if (!ReturnCode.isSuccess(await session.getReturnCode())) {
        throw StateError('音视频转换失败，请检查文件是否损坏');
      }
      final modelDirectory = await getApplicationSupportDirectory();
      final model = File(p.join(modelDirectory.path, 'ggml-base.bin'));
      if (!await model.exists()) {
        final data = await rootBundle.load('assets/ggml-base.bin');
        await model.writeAsBytes(
          data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
          flush: true,
        );
      }
      if (mounted) setState(() => status = '正在手机本地识别，请保持应用在前台…');
      final whisper = Whisper(
        model: WhisperModel.base,
        modelDir: modelDirectory.path,
      );
      final response = await whisper.transcribe(
        transcribeRequest: TranscribeRequest(
          audio: wav.path,
          isTranslate: false,
          isNoTimestamps: false,
          language: 'auto',
        ),
      );
      final text = response.text;
      if (text.trim().isEmpty) throw StateError('未识别到语音，请检查音频');
      final segments = response.segments;
      final srt = segments == null
          ? ''
          : segments
                .asMap()
                .entries
                .map(
                  (e) =>
                      '${e.key + 1}\n${srtTime(e.value.fromTs.inMilliseconds / 1000)} --> ${srtTime(e.value.toTs.inMilliseconds / 1000)}\n${e.value.text}\n',
                )
                .join('\n');
      if (mounted) {
        setState(() {
          preview = TextPreview(p.basenameWithoutExtension(input.path), [
            TextEntry('', text),
          ], srt: srt);
          status = '识别完成，可选择格式后确认导出';
        });
      }
    } finally {
      if (await wav.exists()) await wav.delete();
    }
  }, '正在准备音频…');
  Future<void> pickFile() async {
    final choice = await FilePicker.pickFile(
      type: FileType.custom,
      allowedExtensions: [
        'mp4',
        'mkv',
        'avi',
        'mov',
        'wmv',
        'webm',
        'mp3',
        'wav',
        'm4a',
        'aac',
        'flac',
        'ogg',
      ],
    );
    if (choice?.path != null && mounted) {
      setState(() {
        media = File(choice!.path!);
        preview = null;
        status = '文件已就绪';
      });
    }
  }

  Future<void> login() async {
    await Navigator.push(
      context,
      MaterialPageRoute<void>(builder: (_) => const LoginPage()),
    );
    final cookies = await CookieManager.instance().getCookies(
      url: WebUri('https://www.bilibili.com/'),
    );
    service.biliCookies = cookies
        .map((cookie) => '${cookie.name}=${cookie.value}')
        .join('; ');
    await (await SharedPreferences.getInstance()).setString(
      'biliCookies',
      service.biliCookies,
    );
    if (mounted) {
      setState(
        () => status = service.biliCookies.contains('SESSDATA=')
            ? 'B站已登录，请重新获取'
            : '登录尚未完成，可以继续浏览后再试',
      );
    }
  }

  Future<void> share(File file) async {
    if (!await file.exists()) {
      if (mounted) setState(() => status = '文件已被移动或删除');
      return;
    }
    try {
      await SharePlus.instance.share(
        ShareParams(
          files: [XFile(file.path)],
          title: '分享至微信 / QQ',
          subject: p.basename(file.path),
        ),
      );
    } catch (error) {
      if (mounted) setState(() => status = '分享失败：$error');
    }
  }

  Future<void> open(File file) async {
    final result = await OpenFilex.open(file.path);
    if (result.type != ResultType.done && mounted) {
      setState(() => status = '无法打开：${result.message}，可使用分享发送到其他应用');
    }
  }

  Widget card(List<Widget> children) => Card(
    elevation: 0,
    color: Colors.white,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(18),
      side: const BorderSide(color: Color(0xffdfeafd)),
    ),
    child: Padding(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: children,
      ),
    ),
  );
  Widget button(
    String label,
    VoidCallback action, {
    IconData icon = Icons.play_arrow_rounded,
  }) => FilledButton.icon(
    onPressed: busy ? null : action,
    icon: Icon(icon),
    label: Text(label),
    style: FilledButton.styleFrom(
      backgroundColor: blue,
      minimumSize: const Size(double.infinity, 46),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
    ),
  );
  Widget formatPicker() => Wrap(
    spacing: 6,
    runSpacing: 0,
    children: ExportFormat.values
        .map(
          (value) => ChoiceChip(
            label: Text(switch (value) {
              ExportFormat.txt => 'TXT',
              ExportFormat.md => 'MD',
              ExportFormat.docx => 'Word',
              ExportFormat.pdf => 'PDF',
            }),
            selected: format == value,
            onSelected: busy ? null : (_) => setState(() => format = value),
          ),
        )
        .toList(),
  );
  @override
  Widget build(BuildContext context) {
    const names = ['下载字幕', '下载视频', '音视频转文字', '知乎文本'];
    return Scaffold(
      appBar: AppBar(
        backgroundColor: const Color(0xfff0f6ff),
        title: Row(
          children: [
            Image.asset('assets/logo.png', width: 36, height: 36),
            const SizedBox(width: 10),
            const Text('拾文', style: TextStyle(fontWeight: FontWeight.bold)),
          ],
        ),
        actions: [
          if (mode < 2)
            TextButton(
              onPressed: busy ? null : login,
              child: Text(
                service.biliCookies.contains('SESSDATA=') ? 'B站已登录' : 'B站登录',
              ),
            ),
          IconButton(
            onPressed: busy
                ? null
                : () => Navigator.push(
                    context,
                    MaterialPageRoute<void>(builder: (_) => const FilesPage()),
                  ),
            tooltip: '导出文件',
            icon: const Icon(Icons.folder_open_rounded),
          ),
        ],
      ),
      body: SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
          child: Column(
            children: [
              card([
                Text(
                  names[mode],
                  style: const TextStyle(
                    fontSize: 20,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(height: 12),
                if (mode != 2)
                  TextField(
                    controller: link,
                    enabled: !busy,
                    onChanged: (_) => invalidate(),
                    onSubmitted: (_) => fetch(),
                    maxLines: 2,
                    minLines: 1,
                    keyboardType: TextInputType.url,
                    decoration: InputDecoration(
                      hintText: mode == 3
                          ? '粘贴知乎问题、回答或专栏链接'
                          : '粘贴 B站视频链接或 BV 号',
                      suffixIcon: IconButton(
                        tooltip: '粘贴链接',
                        onPressed: busy
                            ? null
                            : () async {
                                final value = await Clipboard.getData(
                                  Clipboard.kTextPlain,
                                );
                                link.text = value?.text ?? '';
                                invalidate();
                              },
                        icon: const Icon(Icons.content_paste_rounded),
                      ),
                    ),
                  ),
                if (mode == 2)
                  Container(
                    padding: const EdgeInsets.all(12),
                    decoration: BoxDecoration(
                      color: const Color(0xfff0f6ff),
                      borderRadius: BorderRadius.circular(12),
                      border: Border.all(color: const Color(0xffc9defe)),
                    ),
                    child: Column(
                      children: [
                        const Icon(
                          Icons.audio_file_rounded,
                          size: 36,
                          color: blue,
                        ),
                        Text(
                          media == null ? '选择音频或视频文件' : p.basename(media!.path),
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                        ),
                        TextButton.icon(
                          onPressed: busy ? null : pickFile,
                          icon: const Icon(Icons.folder_open_rounded),
                          label: Text(media == null ? '选择文件' : '更换文件'),
                        ),
                      ],
                    ),
                  ),
                if (mode == 3) ...[
                  const SizedBox(height: 8),
                  SegmentedButton<bool>(
                    segments: const [
                      ButtonSegment(value: true, label: Text('整个问题')),
                      ButtonSegment(value: false, label: Text('当前回答')),
                    ],
                    selected: {allAnswers},
                    onSelectionChanged: busy
                        ? null
                        : (values) {
                            setState(() => allAnswers = values.first);
                            invalidate();
                          },
                  ),
                  const SizedBox(height: 12),
                  if (preview == null) button('获取内容', fetch),
                  if (preview != null) ...[
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            '已获取 ${preview!.entries.length} 条${preview!.isQuestion ? '回答' : '内容'}',
                            style: const TextStyle(
                              fontWeight: FontWeight.bold,
                              fontSize: 18,
                              color: blue,
                            ),
                          ),
                        ),
                        TextButton(
                          onPressed: busy ? null : fetch,
                          child: const Text('重新获取'),
                        ),
                      ],
                    ),
                    Text(
                      preview!.title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    if (preview!.isQuestion)
                      Padding(
                        padding: const EdgeInsets.only(top: 10),
                        child: Row(
                          children: [
                            const Text('从 '),
                            Expanded(
                              child: TextField(
                                controller: first,
                                enabled: !busy,
                                keyboardType: TextInputType.number,
                                decoration: const InputDecoration(
                                  isDense: true,
                                ),
                              ),
                            ),
                            const Text(' 到 '),
                            Expanded(
                              child: TextField(
                                controller: last,
                                enabled: !busy,
                                keyboardType: TextInputType.number,
                                decoration: const InputDecoration(
                                  isDense: true,
                                ),
                              ),
                            ),
                            const Text(' 条'),
                          ],
                        ),
                      ),
                    formatPicker(),
                    button('确认导出', save, icon: Icons.file_download_outlined),
                  ],
                ],
                if (mode == 2) ...[
                  const SizedBox(height: 8),
                  formatPicker(),
                  if (preview == null)
                    button('开始识别', transcribe, icon: Icons.graphic_eq_rounded)
                  else ...[
                    const Text(
                      '识别完成',
                      style: TextStyle(
                        color: blue,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(height: 8),
                    button('确认导出', save, icon: Icons.file_download_outlined),
                    TextButton(
                      onPressed: busy ? null : transcribe,
                      child: const Text('重新识别'),
                    ),
                  ],
                  const SizedBox(height: 6),
                  const Text(
                    '在手机本地识别，无需电脑。处理期间请保持应用在前台。',
                    style: TextStyle(fontSize: 12, color: Colors.blueGrey),
                  ),
                ],
                if (mode < 2) ...[
                  const SizedBox(height: 8),
                  button(video == null ? '获取内容' : '重新获取', fetch),
                  if (video != null) ...[
                    const SizedBox(height: 10),
                    Text(
                      video!.title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontWeight: FontWeight.bold),
                    ),
                    if (video!.pages.length > 1)
                      DropdownButton<int>(
                        isExpanded: true,
                        value: page,
                        items: video!.pages
                            .asMap()
                            .entries
                            .map(
                              (e) => DropdownMenuItem(
                                value: e.key,
                                child: Text(
                                  'P${e.key + 1} ${e.value['part']}',
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                            )
                            .toList(),
                        onChanged: busy
                            ? null
                            : (value) {
                                if (value != null) {
                                  setState(() => page = value);
                                  runTask(fetchBiliOptions, '正在获取分集…');
                                }
                              },
                      ),
                    if (mode == 0 && tracks.isNotEmpty) ...[
                      DropdownButton<int>(
                        isExpanded: true,
                        value: track,
                        items: tracks
                            .asMap()
                            .entries
                            .map(
                              (e) => DropdownMenuItem(
                                value: e.key,
                                child: Text('${e.value['lan_doc']}'),
                              ),
                            )
                            .toList(),
                        onChanged: busy
                            ? null
                            : (value) => setState(() => track = value ?? 0),
                      ),
                      formatPicker(),
                      button('确认导出', save, icon: Icons.file_download_outlined),
                    ],
                    if (mode == 1 && streams.isNotEmpty) ...[
                      DropdownButton<int>(
                        isExpanded: true,
                        value: quality,
                        items: streams
                            .asMap()
                            .entries
                            .map(
                              (e) => DropdownMenuItem(
                                value: e.key,
                                child: Text(qualityName(e.value['id'] as int)),
                              ),
                            )
                            .toList(),
                        onChanged: busy
                            ? null
                            : (value) => setState(() => quality = value ?? 0),
                      ),
                      button('下载视频', download, icon: Icons.download_rounded),
                    ],
                  ],
                ],
              ]),
              Card(
                elevation: 0,
                color: const Color(0xffe0edff),
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          if (busy)
                            const SizedBox(
                              width: 18,
                              height: 18,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          else
                            const Icon(
                              Icons.info_outline_rounded,
                              size: 18,
                              color: blue,
                            ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              status,
                              style: const TextStyle(fontSize: 13),
                            ),
                          ),
                        ],
                      ),
                      if (progress != null) ...[
                        const SizedBox(height: 8),
                        LinearProgressIndicator(value: progress),
                      ],
                      for (final file in recent.take(2))
                        Padding(
                          padding: const EdgeInsets.only(top: 8),
                          child: Row(
                            children: [
                              Expanded(
                                child: Text(
                                  p.basename(file.path),
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(fontSize: 12),
                                ),
                              ),
                              IconButton(
                                onPressed: busy ? null : () => open(file),
                                tooltip: '打开文件',
                                icon: const Icon(
                                  Icons.open_in_new_rounded,
                                  size: 20,
                                ),
                              ),
                              IconButton(
                                onPressed: busy ? null : () => share(file),
                                tooltip: '分享至微信 / QQ',
                                icon: const Icon(Icons.share_rounded, size: 20),
                              ),
                            ],
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: mode,
        onDestinationSelected: busy
            ? null
            : (value) {
                setState(() => mode = value);
                invalidate();
              },
        destinations: const [
          NavigationDestination(
            icon: Icon(Icons.subtitles_outlined),
            label: '字幕',
          ),
          NavigationDestination(
            icon: Icon(Icons.video_library_outlined),
            label: '视频',
          ),
          NavigationDestination(
            icon: Icon(Icons.graphic_eq_rounded),
            label: '转文字',
          ),
          NavigationDestination(
            icon: Icon(Icons.article_outlined),
            label: '知乎',
          ),
        ],
      ),
    );
  }
}

String qualityName(int quality) =>
    {
      127: '8K',
      126: '杜比视界',
      125: 'HDR',
      120: '4K',
      116: '1080P 60帧',
      112: '1080P 高码率',
      100: '智能修复',
      80: '1080P',
      74: '720P 60帧',
      64: '720P',
      32: '480P',
      16: '360P',
    }[quality] ??
    '画质 $quality';

class LoginPage extends StatelessWidget {
  const LoginPage({super.key});
  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: const Text('B站登录'),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('完成'),
        ),
      ],
    ),
    body: InAppWebView(
      initialUrlRequest: URLRequest(
        url: WebUri('https://passport.bilibili.com/login'),
      ),
      initialSettings: InAppWebViewSettings(
        javaScriptEnabled: true,
        thirdPartyCookiesEnabled: true,
        sharedCookiesEnabled: true,
      ),
    ),
  );
}

class FilesPage extends StatefulWidget {
  const FilesPage({super.key});
  @override
  State<FilesPage> createState() => _FilesState();
}

class _FilesState extends State<FilesPage> {
  late final Future<List<File>> files = load();
  Future<List<File>> load() async {
    final dir = await outputDirectory();
    final list = await dir
        .list()
        .where((e) => e is File && !e.path.endsWith('.part'))
        .cast<File>()
        .toList();
    list.sort((a, b) => b.lastModifiedSync().compareTo(a.lastModifiedSync()));
    return list;
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: const Text('导出文件'),
      actions: [
        IconButton(
          tooltip: '关于拾文',
          icon: const Icon(Icons.info_outline),
          onPressed: () => showAboutDialog(
            context: context,
            applicationName: '拾文',
            applicationVersion: '1.0.0',
            applicationIcon: Image.asset(
              'assets/logo.png',
              width: 48,
              height: 48,
            ),
          ),
        ),
      ],
    ),
    body: FutureBuilder<List<File>>(
      future: files,
      builder: (context, snapshot) {
        if (snapshot.hasError) {
          return Center(child: Text('读取失败：${snapshot.error}'));
        }
        if (!snapshot.hasData) {
          return const Center(child: CircularProgressIndicator());
        }
        if (snapshot.data!.isEmpty) {
          return const Center(child: Text('导出的文件会显示在这里'));
        }
        return ListView.separated(
          itemCount: snapshot.data!.length,
          separatorBuilder: (_, _) => const Divider(height: 1),
          itemBuilder: (context, index) {
            final file = snapshot.data![index];
            return ListTile(
              title: Text(p.basename(file.path)),
              subtitle: Text(
                '${(file.lengthSync() / 1024).toStringAsFixed(1)} KB',
              ),
              onTap: () => OpenFilex.open(file.path),
              trailing: PopupMenuButton<String>(
                onSelected: (value) async {
                  try {
                    if (value == 'share') {
                      await SharePlus.instance.share(
                        ShareParams(
                          files: [XFile(file.path)],
                          title: '分享至微信 / QQ',
                          subject: p.basename(file.path),
                        ),
                      );
                    }
                    if (value == 'save') {
                      await saveToPhone(file);
                    }
                  } catch (error) {
                    if (context.mounted) {
                      ScaffoldMessenger.of(
                        context,
                      ).showSnackBar(SnackBar(content: Text('操作失败：$error')));
                    }
                  }
                },
                itemBuilder: (_) => const [
                  PopupMenuItem(value: 'share', child: Text('分享至微信 / QQ')),
                  PopupMenuItem(value: 'save', child: Text('另存到手机目录')),
                ],
              ),
            );
          },
        );
      },
    ),
  );
}

Future<String?> saveToPhone(File file) async {
  const storage = MethodChannel('cn.shiwen/storage');
  final mime =
      {
        '.txt': 'text/plain',
        '.md': 'text/markdown',
        '.srt': 'application/x-subrip',
        '.pdf': 'application/pdf',
        '.docx':
            'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        '.mp4': 'video/mp4',
      }[p.extension(file.path)] ??
      'application/octet-stream';
  return storage.invokeMethod<String>('saveFile', {
    'path': file.path,
    'mime': mime,
  });
}
