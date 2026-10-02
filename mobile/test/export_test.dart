import 'dart:io';
import 'dart:convert';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path_provider_platform_interface/path_provider_platform_interface.dart';
import 'package:shiwen_mobile/models.dart';
import 'package:shiwen_mobile/services/exports.dart';
import 'fake_paths.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test('四种格式可以导出中文，PDF 支持长段落', () async {
    final temp = Directory.systemTemp.createTempSync('shiwen-export-test');
    PathProviderPlatform.instance = TestPaths(temp.path);
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('plugins.flutter.io/path_provider'),
          (_) async => temp.path,
        );
    try {
      final preview = TextPreview('中文测试', [TextEntry('', '这是中文长段落。' * 400)]);
      for (final format in ExportFormat.values) {
        final file = await exportPreview(preview, 1, 1, format);
        final bytes = await file.readAsBytes();
        expect(bytes.length, greaterThan(20));
        if (format == ExportFormat.pdf) {
          expect(ascii.decode(bytes.take(4).toList()), '%PDF');
        }
        if (format == ExportFormat.txt) {
          expect(utf8.decode(bytes), contains('这是中文长段落。'));
        }
      }
    } finally {
      temp.deleteSync(recursive: true);
    }
  });
}
