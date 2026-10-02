import 'dart:io';
import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path_provider_platform_interface/path_provider_platform_interface.dart';
import 'package:shiwen_mobile/services/content_service.dart';
import 'package:shiwen_mobile/services/exports.dart';
import 'package:shiwen_mobile/services/zhihu_sign.dart';
import 'fake_paths.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const url =
      'https://zhuanlan.zhihu.com/p/2079159112412282919?share_code=test';
  ContentService mockService(
    Map<String, dynamic> Function(RequestOptions) respond,
  ) {
    final dio = Dio();
    dio.interceptors.add(
      InterceptorsWrapper(
        onRequest: (request, handler) {
          handler.resolve(
            Response(
              requestOptions: request,
              data: respond(request),
              statusCode: 200,
            ),
          );
        },
      ),
    );
    return ContentService(client: dio);
  }

  test('知乎标记节选时禁止将其作为可导出的全文', () async {
    final service = mockService(
      (_) => {
        'title': '文章',
        'content': '<p>只到一半的正文</p>',
        'content_need_truncated': true,
        'force_login_when_click_read_more': true,
      },
    );
    await expectLater(
      service.zhihu(url, true),
      throwsA(
        isA<ZhihuIncompleteContentException>().having(
          (e) => e.message,
          '登录提示',
          contains('知乎登录'),
        ),
      ),
    );
    service.zhihuCookies = 'z_c0=mock-session';
    await expectLater(
      service.zhihu(url, true),
      throwsA(isA<ZhihuIncompleteContentException>()),
    );
  });

  test('登录后的完整长文章可导出末尾，登录会话与签名一致', () async {
    const fingerprint = '"mock-fingerprint|123"';
    const cookie = 'z_c0=mock-session; d_c0=$fingerprint';
    final longText = '${'完整段落。' * 300}这是文章最后一段。';
    final service = mockService((request) {
      expect(request.headers['Cookie'], cookie);
      expect(
        request.headers['x-zse-96'],
        signZhihu(request.uri.toString(), fingerprint)['x-zse-96'],
      );
      return {
        'title': '完整文章',
        'content': '<p>$longText</p>',
        'content_need_truncated': false,
      };
    })..zhihuCookies = cookie;
    final preview = await service.zhihu(url, true);
    expect(preview.entries.single.text, longText);
    final temp = Directory.systemTemp.createTempSync('shiwen-complete-article');
    PathProviderPlatform.instance = TestPaths(temp.path);
    try {
      final file = await exportPreview(preview, 1, 1, ExportFormat.txt);
      final saved = await file.readAsString();
      expect(saved, '完整文章\n\n$longText');
      expect(saved.endsWith('这是文章最后一段。'), isTrue);
    } finally {
      temp.deleteSync(recursive: true);
    }
  });

  test('单回答和分页中的回答同样拒绝节选', () async {
    final single = mockService(
      (_) => {'content': '<p>部分回答</p>', 'content_need_truncated': true},
    );
    await expectLater(
      single.zhihu('https://www.zhihu.com/question/123/answer/456', false),
      throwsA(isA<ZhihuIncompleteContentException>()),
    );
    var calls = 0;
    final question = mockService((_) {
      calls++;
      return {
        'data': [
          {
            'id': '$calls',
            'content': '<p>回答内容</p>',
            'content_need_truncated': calls == 2,
          },
        ],
        'paging': {
          'is_end': calls == 2,
          'next': 'https://api.zhihu.com/v4/questions/123/answers?offset=1',
        },
      };
    });
    await expectLater(
      question.zhihu('https://www.zhihu.com/question/123', true),
      throwsA(isA<ZhihuIncompleteContentException>()),
    );
    expect(calls, 2);
  });
}
