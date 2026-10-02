import 'package:path_provider_platform_interface/path_provider_platform_interface.dart';

class TestPaths extends PathProviderPlatform {
  TestPaths(this.root);
  final String root;
  @override
  Future<String?> getApplicationDocumentsPath() async => root;
  @override
  Future<String?> getTemporaryPath() async => root;
  @override
  Future<String?> getApplicationSupportPath() async => root;
}
