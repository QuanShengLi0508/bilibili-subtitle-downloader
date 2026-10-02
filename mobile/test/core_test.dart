import 'dart:convert';
import 'package:archive/archive.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shiwen_mobile/models.dart';
import 'package:shiwen_mobile/services/content_service.dart';
import 'package:shiwen_mobile/services/exports.dart';
import 'package:shiwen_mobile/services/zhihu_sign.dart';

void main() {
  test('包含短链字样的其他网站不能接收 B站登录信息', () async {
    final service = ContentService()..biliCookies = 'SESSDATA=test';
    await expectLater(service.bili('https://example.invalid/?url=b23.tv'), throwsArgumentError);
  });
  test('安卓长中文文件名保留完整字符并限制字节长度', () {
    expect(utf8.encode(safeName('中文标题' * 50)).length, lessThanOrEqualTo(192));
    expect(safeName('😀' * 100).runes.length, 48);
    expect(safeName(''), '拾文导出');
  });
  test('签名与已验证的电脑版一致', () {
    const url =
        'https://api.zhihu.com/v4/questions/2059626138549350826/answers?include=content,author,question&limit=20&offset=0';
    final headers = signZhihu(url, 'ZhihuAnonymousFingerprint00000000000000');
    expect(
      headers['x-zse-96'],
      '2.0_XOR6sF3xcKRNq3GVb0+yBNkc1xHSJXZ6sG7==I/RGCOT5Pq2y1qLdjRiQZLku9RT',
    );
  });
  test('导出范围含首尾，不带其他回答', () {
    final preview = TextPreview(
      '问题',
      List.generate(4, (i) => TextEntry('作者${i + 1}', '正文${i + 1}')),
      isQuestion: true,
    );
    final body = preview.rangeBody(2, 3);
    expect(body, contains('正文2'));
    expect(body, contains('正文3'));
    expect(body, isNot(contains('正文1')));
    expect(body, isNot(contains('正文4')));
    expect(() => preview.rangeBody(0, 2), throwsArgumentError);
    expect(() => preview.rangeBody(2, 5), throwsArgumentError);
    expect(() => preview.rangeBody(3, 2), throwsArgumentError);
  });
  test('Word 文档含有效结构和转义后的中文正文', () {
    final bytes = docxBytes('中文标题', '甲 & 乙 < 丙\n第二段');
    final archive = ZipDecoder().decodeBytes(bytes);
    final content = archive.findFile('word/document.xml')!;
    final xml = utf8.decode(content.content as List<int>);
    expect(xml, contains('中文标题'));
    expect(xml, contains('甲 &amp; 乙 &lt; 丙'));
    expect(xml, contains('第二段'));
    expect(archive.findFile('_rels/.rels'), isNotNull);
  });
  test('正文保留段落且忽略网页脚本', () {
    expect(
      plainHtml('<p>第一段</p><p>第二段<br>换行</p><script>无关内容</script>'),
      '第一段\n\n第二段\n换行',
    );
    expect(srtTime(62.125), '00:01:02,125');
  });
  test(
    '真实链接可以直接在手机服务获取更多回答',
    () async {
      final preview = await ContentService().zhihu(
        'https://www.zhihu.com/question/2059626138549350826/answer/2072289562546774195',
        true,
      );
      expect(preview.entries.length, greaterThan(1));
      expect(preview.rangeBody(2, 3), contains('回答 3'));
    },
    tags: ['network'],
    skip: const bool.fromEnvironment('SKIP_NETWORK', defaultValue: true),
  );
}
