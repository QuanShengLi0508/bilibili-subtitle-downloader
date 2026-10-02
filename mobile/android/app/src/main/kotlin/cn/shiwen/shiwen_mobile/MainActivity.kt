package cn.shiwen.shiwen_mobile

import android.app.Activity
import android.content.Intent
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

class MainActivity : FlutterActivity() {
    private var pendingSave: MethodChannel.Result? = null
    private var sourceFile: File? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "cn.shiwen/storage")
            .setMethodCallHandler { call, result ->
                if (call.method != "saveFile") {
                    result.notImplemented()
                } else if (pendingSave != null) {
                    result.error("busy", "请先关闭当前保存窗口", null)
                } else {
                    val path = call.argument<String>("path") ?: ""
                    val file = File(path)
                    if (!file.isFile) {
                        result.error("missing", "文件不存在", null)
                    } else {
                        sourceFile = file
                        pendingSave = result
                        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT)
                            .addCategory(Intent.CATEGORY_OPENABLE)
                            .setType(call.argument<String>("mime") ?: "application/octet-stream")
                            .putExtra(Intent.EXTRA_TITLE, file.name)
                        try { startActivityForResult(intent, 6020) }
                        catch (error: Exception) {
                            pendingSave = null
                            sourceFile = null
                            result.error("save", error.message, null)
                        }
                    }
                }
            }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != 6020) return
        val result = pendingSave ?: return
        val file = sourceFile
        pendingSave = null
        sourceFile = null
        val uri = data?.data
        if (resultCode != Activity.RESULT_OK || uri == null || file == null) {
            result.success(null)
            return
        }
        Thread {
            try {
                contentResolver.openOutputStream(uri, "w").use { output ->
                    requireNotNull(output) { "所选位置不可写入" }
                    file.inputStream().use { input -> input.copyTo(output) }
                }
                runOnUiThread { result.success(uri.toString()) }
            } catch (error: Exception) {
                runOnUiThread { result.error("save", error.message, null) }
            }
        }.start()
    }
}
