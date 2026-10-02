import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:path_provider_platform_interface/path_provider_platform_interface.dart';
import 'package:shiwen_mobile/main.dart';
import 'fake_paths.dart';

void main() {
  testWidgets('手机小屏可以切换四种功能并显示操作按钮', (tester) async {
    SharedPreferences.setMockInitialValues({});
    final temp = Directory.systemTemp.createTempSync('shiwen-ui-test');
    PathProviderPlatform.instance = TestPaths(temp.path);
    final output = Directory('${temp.path}/拾文导出')..createSync();
    final document = File('${output.path}/分享测试.pdf')
      ..writeAsStringSync('%PDF-test');
    MethodCall? shared;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('dev.fluttercommunity.plus/share'),
          (call) async {
            shared = call;
            return 'com.tencent.mm';
          },
        );
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('plugins.flutter.io/path_provider'),
          (_) async => temp.path,
        );
    tester.view.physicalSize = const Size(360, 740);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(const ShiwenApp());
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 100)),
    );
    await tester.pumpAndSettle();
    expect(find.text('拾文'), findsOneWidget);
    for (final name in ['视频', '转文字', '知乎', '字幕']) {
      await tester.tap(find.text(name));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      if (name == '知乎') expect(find.text('知乎登录'), findsOneWidget);
    }
    expect(find.text('获取内容'), findsOneWidget);
    final dynamic home = tester.state(find.byType(HomePage));
    await home.remember([document]);
    await tester.pumpAndSettle();
    expect(find.byTooltip('分享至微信 / QQ'), findsOneWidget);
    await tester.runAsync(() async {
      await tester.tap(find.byTooltip('分享至微信 / QQ'));
      await Future<void>.delayed(const Duration(milliseconds: 100));
    });
    expect(shared?.method, 'share');
    expect(shared?.arguments['paths'], [document.path]);
    expect(shared?.arguments['mimeTypes'], ['application/pdf']);
    await tester.pumpWidget(const SizedBox());
    temp.deleteSync(recursive: true);
  });
}
