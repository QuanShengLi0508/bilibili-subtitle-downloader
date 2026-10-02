import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:dio/dio.dart';
import 'package:html/parser.dart' as html;
import 'package:path/path.dart' as p;
import 'package:ffmpeg_kit_flutter_new_min/ffmpeg_kit.dart';
import 'package:ffmpeg_kit_flutter_new_min/return_code.dart';
import '../models.dart';
import 'exports.dart';
import 'zhihu_sign.dart';

const userAgent =
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36 Edg/123.0.0.0';

class ZhihuIncompleteContentException implements Exception {
  const ZhihuIncompleteContentException(this.message);
  final String message;
  @override
  String toString() => message;
}

class ContentService {
  ContentService({Dio? client})
    : dio =
          client ??
          Dio(
            BaseOptions(
              connectTimeout: const Duration(seconds: 20),
              receiveTimeout: const Duration(minutes: 3),
            ),
          );
  final Dio dio;
  String biliCookies = '';
  String zhihuCookies = '';
  bool get zhihuLoggedIn => RegExp(r'(?:^|;\s*)z_c0=').hasMatch(zhihuCookies);
  final CancelToken cancelToken = CancelToken();
  void cancel() => cancelToken.cancel('已取消');
  Future<Map<String, dynamic>> json(
    String url, {
    Map<String, String>? headers,
  }) async {
    final response = await dio.get<dynamic>(
      url,
      options: Options(headers: {'User-Agent': userAgent, ...?headers}),
      cancelToken: cancelToken,
    );
    final data = response.data is String
        ? jsonDecode(response.data as String)
        : response.data;
    if (data is! Map<String, dynamic>) throw StateError('接口未返回有效内容');
    if (data['error'] != null) throw StateError('平台暂时无法提供内容：${data['error']}');
    return data;
  }

  Map<String, String> get biliHeaders => {
    'Referer': 'https://www.bilibili.com/',
    if (biliCookies.isNotEmpty) 'Cookie': biliCookies,
  };
  Future<TextPreview> zhihu(
    String input,
    bool allAnswers, {
    void Function(int)? onCount,
  }) async {
    final uri = Uri.tryParse(
      input.trim().startsWith('http')
          ? input.trim()
          : 'https://${input.trim()}',
    );
    if (uri == null ||
        !(uri.host == 'zhihu.com' || uri.host.endsWith('.zhihu.com'))) {
      throw ArgumentError('请粘贴知乎问题、回答或专栏链接');
    }
    final answerId = RegExp(r'/answer/(\d+)').firstMatch(uri.path)?.group(1);
    var questionId = RegExp(r'/question/(\d+)').firstMatch(uri.path)?.group(1);
    final articleId = RegExp(
      r'/(?:p|article)/(\d+)',
    ).firstMatch(uri.path)?.group(1);
    Future<Map<String, dynamic>> get(String url) async {
      final savedFingerprint = RegExp(
        r'(?:^|;\s*)d_c0=([^;]+)',
      ).firstMatch(zhihuCookies)?.group(1);
      final fingerprint =
          savedFingerprint ?? 'ZhihuAnonymousFingerprint00000000000000';
      final cookies = zhihuCookies.isEmpty
          ? 'd_c0=$fingerprint'
          : '$zhihuCookies${savedFingerprint == null ? '; d_c0=$fingerprint' : ''}';
      return json(
        url,
        headers: {
          'Cookie': cookies,
          'Referer': 'https://zhuanlan.zhihu.com/',
          'Origin': 'https://zhuanlan.zhihu.com',
          'Accept': '*/*',
          'Accept-Language': 'zh-CN,zh;q=0.9',
          ...signZhihu(url, fingerprint),
        },
      );
    }

    if (articleId != null) {
      final body = await get(
        'https://zhuanlan.zhihu.com/api/articles/$articleId',
      );
      requireCompleteZhihuContent(body);
      return TextPreview('${body['title'] ?? '知乎专栏'}', [
        TextEntry(
          '',
          plainHtml('${body['content'] ?? body['content_html'] ?? ''}'),
        ),
      ]);
    }
    if (answerId != null && (!allAnswers || questionId == null)) {
      final body = await get(
        'https://api.zhihu.com/v4/answers/$answerId?include=content,author,question',
      );
      if (!allAnswers) {
        requireCompleteZhihuContent(body);
        return TextPreview('${body['question']?['title'] ?? '知乎回答'}', [
          TextEntry(
            '${body['author']?['name'] ?? '知乎用户'}',
            plainHtml('${body['content'] ?? ''}'),
            url: uri.toString(),
          ),
        ]);
      }
      questionId = '${body['question']?['id'] ?? ''}';
    }
    if (questionId == null || questionId.isEmpty) {
      throw ArgumentError('无法识别所属问题');
    }
    var endpoint =
        'https://api.zhihu.com/v4/questions/$questionId/answers?include=content,author,question&limit=20&offset=0';
    final visited = <String>{};
    final ids = <String>{};
    final entries = <TextEntry>[];
    var title = '知乎问题';
    while (true) {
      if (!visited.add(endpoint)) throw StateError('分页重复，未能获取完整列表，请重试');
      final body = await get(endpoint);
      final before = entries.length;
      final data = body['data'];
      if (data is! List) throw StateError('平台未返回回答列表');
      for (final item in data) {
        final id = '${item['id'] ?? ''}';
        if (id.isEmpty) throw StateError('回答缺少编号');
        if (!ids.add(id)) continue;
        requireCompleteZhihuContent(Map<String, dynamic>.from(item as Map));
        title = '${item['question']?['title'] ?? title}';
        entries.add(
          TextEntry(
            '${item['author']?['name'] ?? '知乎用户'}',
            plainHtml('${item['content'] ?? item['content_html'] ?? ''}'),
            url: 'https://www.zhihu.com/question/$questionId/answer/$id',
          ),
        );
      }
      onCount?.call(entries.length);
      if (body['paging']?['is_end'] == true) break;
      if (before == entries.length) throw StateError('平台未提供后续回答，请重试');
      final next = '${body['paging']?['next'] ?? ''}';
      if (next.isEmpty) throw StateError('回答分页地址缺失');
      final nextUri = Uri.parse(endpoint).resolve(next);
      if (!(nextUri.host == 'zhihu.com' ||
              nextUri.host.endsWith('.zhihu.com')) ||
          !['http', 'https'].contains(nextUri.scheme)) {
        throw StateError('分页地址不正确');
      }
      endpoint = nextUri.toString();
    }
    if (entries.isEmpty) throw StateError('没有可获取的回答');
    return TextPreview(title, entries, isQuestion: true);
  }

  void requireCompleteZhihuContent(Map<String, dynamic> body) {
    const fields = [
      'content_need_truncated',
      'is_truncated',
      'is_content_truncated',
      'content_is_truncated',
    ];
    if (fields.any(
      (key) => body[key] == true || body[key] == 1 || body[key] == 'true',
    )) {
      throw ZhihuIncompleteContentException(
        zhihuLoggedIn
            ? '知乎仍只返回节选，未获取全文。请重新登录知乎，并确认账号可阅读全文后再获取；本次不会导出。'
            : '知乎只返回了节选。请点右上角「知乎登录」，登录完成后重新获取；未获取全文，本次不会导出。',
      );
    }
  }

  Future<BiliVideo> bili(String input) async {
    var text = input.trim();
    if (text.contains('b23.tv')) {
      final url = RegExp(r'https?://[^\s]+').firstMatch(text)?.group(0);
      if (url == null) throw ArgumentError('短链接不正确');
      final shortUri = Uri.parse(url);
      if (shortUri.host != 'b23.tv' && shortUri.host != 'www.b23.tv') {
        throw ArgumentError('请粘贴 b23.tv 的 B站短链接');
      }
      final response = await dio.get<String>(
        url,
        options: Options(
          headers: {
            'User-Agent': userAgent,
            'Referer': 'https://www.bilibili.com/',
          },
          responseType: ResponseType.plain,
        ),
        cancelToken: cancelToken,
      );
      text = response.realUri.toString();
    }
    final bv = RegExp(r'BV[a-zA-Z0-9]{10}').firstMatch(text)?.group(0);
    final av = RegExp(
      r'(?:av|/av)(\d+)',
      caseSensitive: false,
    ).firstMatch(text)?.group(1);
    if (bv == null && av == null) throw ArgumentError('请粘贴 B站视频链接或 BV/AV 号');
    final body = await json(
      'https://api.bilibili.com/x/web-interface/view?${bv != null ? 'bvid=$bv' : 'aid=$av'}',
      headers: biliHeaders,
    );
    if (body['code'] != 0) throw StateError('${body['message'] ?? '无法获取视频'}');
    final data = body['data'];
    return BiliVideo(
      '${data['title']}',
      '${data['bvid']}',
      (data['pages'] as List)
          .map((page) => Map<String, dynamic>.from(page as Map))
          .toList(),
    );
  }

  Future<String> wbiQuery(Map<String, String> values) async {
    final nav = await json(
      'https://api.bilibili.com/x/web-interface/nav',
      headers: biliHeaders,
    );
    final image = nav['data']?['wbi_img'];
    String key(dynamic url) =>
        Uri.parse('$url').pathSegments.last.split('.').first;
    final combined = key(image?['img_url']) + key(image?['sub_url']);
    const mixin = [
      46,
      47,
      18,
      2,
      53,
      8,
      23,
      32,
      15,
      50,
      10,
      31,
      58,
      3,
      45,
      35,
      27,
      43,
      5,
      49,
      33,
      9,
      42,
      19,
      29,
      28,
      14,
      39,
      12,
      38,
      41,
      13,
    ];
    if (combined.length < 64) throw StateError('无法获取 B站签名信息');
    final secret = mixin.map((i) => combined[i]).join();
    final params = {
      ...values,
      'wts': '${DateTime.now().millisecondsSinceEpoch ~/ 1000}',
    };
    final keys = params.keys.toList()..sort();
    final query = keys
        .map(
          (k) =>
              '$k=${Uri.encodeComponent(params[k]!.replaceAll(RegExp(r"[!'()*]"), ''))}',
        )
        .join('&');
    return '$query&w_rid=${md5.convert(utf8.encode(query + secret))}';
  }

  Future<List<Map<String, dynamic>>> subtitles(
    BiliVideo video,
    int page,
  ) async {
    final query = await wbiQuery({
      'bvid': video.bvid,
      'cid': '${video.pages[page]['cid']}',
    });
    final response = await json(
      'https://api.bilibili.com/x/player/wbi/v2?$query',
      headers: biliHeaders,
    );
    if (response['code'] != 0) {
      throw StateError('${response['message'] ?? '字幕获取失败'}');
    }
    return ((response['data']?['subtitle']?['subtitles'] ?? []) as List)
        .map((e) => Map<String, dynamic>.from(e as Map))
        .toList();
  }

  Future<TextPreview> subtitleText(
    String title,
    Map<String, dynamic> track,
  ) async {
    var url = '${track['subtitle_url']}';
    if (url.startsWith('//')) url = 'https:$url';
    final response = await json(url, headers: biliHeaders);
    final lines = response['body'] as List;
    final text = lines.map((line) => '${line['content']}').join('\n');
    final srt = lines
        .asMap()
        .entries
        .map(
          (e) =>
              '${e.key + 1}\n${srtTime((e.value['from'] as num).toDouble())} --> ${srtTime((e.value['to'] as num).toDouble())}\n${e.value['content']}\n',
        )
        .join('\n');
    return TextPreview(title, [TextEntry('', text)], srt: srt);
  }

  Future<List<Map<String, dynamic>>> streams(BiliVideo video, int page) async {
    final query = await wbiQuery({
      'bvid': video.bvid,
      'cid': '${video.pages[page]['cid']}',
      'qn': '0',
      'fnval': '4048',
      'fourk': '1',
    });
    final response = await json(
      'https://api.bilibili.com/x/player/wbi/playurl?$query',
      headers: biliHeaders,
    );
    if (response['code'] != 0) {
      throw StateError('${response['message'] ?? '视频地址获取失败'}');
    }
    final data = response['data'];
    if (data['dash'] == null) {
      final durls = (data['durl'] ?? []) as List;
      if (durls.length != 1) throw StateError('当前视频分段格式暂不支持，请选择其他视频');
      return [
        {'id': data['quality'], 'video': durls.first['url'], 'audio': null},
      ];
    }
    final audios = (data['dash']['audio'] ?? []) as List;
    audios.sort(
      (a, b) => (b['bandwidth'] as num).compareTo(a['bandwidth'] as num),
    );
    final audio = audios.isEmpty
        ? null
        : audios.first['base_url'] ?? audios.first['baseUrl'];
    final videos = List<dynamic>.from(data['dash']['video'] as List);
    videos.sort((a, b) {
      final q = (b['id'] as num).compareTo(a['id'] as num);
      return q != 0 ? q : ('${a['codecs']}'.startsWith('avc') ? -1 : 1);
    });
    final ids = <int>{};
    return [
      for (final v in videos)
        if (ids.add(v['id'] as int))
          {
            'id': v['id'],
            'video': v['base_url'] ?? v['baseUrl'],
            'audio': audio,
          },
    ];
  }

  Future<File> downloadVideo(
    String title,
    Map<String, dynamic> stream,
    void Function(double) onProgress,
  ) async {
    final dir = await outputDirectory();
    final stamp = DateTime.now().millisecondsSinceEpoch;
    final result = File(p.join(dir.path, '${safeName(title)}_$stamp.mp4'));
    final videoPart = '${result.path}.video.part';
    final audioPart = '${result.path}.audio.part';
    try {
      await dio.download(
        '${stream['video']}',
        stream['audio'] == null ? result.path : videoPart,
        options: Options(headers: biliHeaders),
        cancelToken: cancelToken,
        onReceiveProgress: (got, total) {
          if (total > 0) {
            onProgress(got / total * (stream['audio'] == null ? 1 : 0.7));
          }
        },
      );
      if (stream['audio'] != null) {
        await dio.download(
          '${stream['audio']}',
          audioPart,
          options: Options(headers: biliHeaders),
          cancelToken: cancelToken,
          onReceiveProgress: (got, total) {
            if (total > 0) onProgress(0.7 + got / total * 0.25);
          },
        );
        final session = await FFmpegKit.executeWithArguments([
          '-y',
          '-i',
          videoPart,
          '-i',
          audioPart,
          '-c',
          'copy',
          '-movflags',
          '+faststart',
          result.path,
        ]);
        if (!ReturnCode.isSuccess(await session.getReturnCode())) {
          throw StateError('视频合并失败');
        }
      }
      onProgress(1);
      return result;
    } catch (_) {
      if (await result.exists()) await result.delete();
      rethrow;
    } finally {
      for (final path in [videoPart, audioPart]) {
        final f = File(path);
        if (await f.exists()) await f.delete();
      }
    }
  }
}

String plainHtml(String source) {
  final document = html.parse(source);
  for (final element in document.querySelectorAll('script,style,noscript')) {
    element.remove();
  }
  for (final element in document.querySelectorAll('br')) {
    element.replaceWith(html.parseFragment('\n'));
  }
  for (final element in document.querySelectorAll(
    'p,div,li,h1,h2,h3,h4,blockquote,pre',
  )) {
    element.append(html.parseFragment('\n\n'));
  }
  final text = (document.body?.text ?? '')
      .replaceAll(RegExp(r'\n[ \t]+'), '\n')
      .replaceAll(RegExp(r'\n{3,}'), '\n\n')
      .trim();
  if (text.isEmpty) throw StateError('未返回正文');
  return text;
}

String srtTime(double seconds) {
  final milliseconds = (seconds * 1000).round();
  String pad(int value, int width) => '$value'.padLeft(width, '0');
  return '${pad(milliseconds ~/ 3600000, 2)}:${pad(milliseconds ~/ 60000 % 60, 2)}:${pad(milliseconds ~/ 1000 % 60, 2)},${pad(milliseconds % 1000, 3)}';
}
