import 'dart:convert';
import 'dart:io';
import 'package:archive/archive.dart';
import 'package:flutter/services.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';
import 'package:pdf/pdf.dart';
import 'package:pdf/widgets.dart' as pw;
import '../models.dart';

enum ExportFormat { txt, md, docx, pdf }

String safeName(String name) {
  final clean = name.replaceAll(RegExp(r'[\\/:*?"<>|\x00-\x1f]'), '_').trim();
  final limited = String.fromCharCodes(clean.runes.take(48));
  return limited.isEmpty || limited == '.' || limited == '..' ? '拾文导出' : limited;
}
Future<Directory> outputDirectory() async {
  final documents = await getApplicationDocumentsDirectory();
  return Directory(p.join(documents.path, '拾文导出')).create(recursive: true);
}

String xmlEscape(String text) => text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
List<int> docxBytes(String title, String body) {
  final archive = Archive();
  void add(String path, String content) {
    final data = utf8.encode(content);
    archive.addFile(ArchiveFile(path, data.length, data));
  }

  add(
    '[Content_Types].xml',
    '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>',
  );
  add(
    '_rels/.rels',
    '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>',
  );
  String paragraph(String line, {bool heading = false}) =>
      '<w:p><w:pPr><w:spacing w:after="120"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Calibri" w:eastAsia="Noto Sans SC"/>${heading ? '<w:b/><w:sz w:val="36"/>' : '<w:sz w:val="22"/>'}</w:rPr><w:t xml:space="preserve">${xmlEscape(line)}</w:t></w:r></w:p>';
  add(
    'word/document.xml',
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${paragraph(title, heading: true)}${body.split('\n').map(paragraph).join()}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134"/></w:sectPr></w:body></w:document>',
  );
  return ZipEncoder().encode(archive);
}

Future<File> exportPreview(
  TextPreview preview,
  int first,
  int last,
  ExportFormat format,
) async {
  final body = preview.rangeBody(first, last);
  final root = await outputDirectory();
  final suffix = preview.isQuestion ? '_回答$first-$last' : '';
  var name = '${safeName(preview.title)}$suffix';
  var file = File(p.join(root.path, '$name.${format.name}'));
  var count = 1;
  while (await file.exists()) {
    file = File(p.join(root.path, '$name (${count++}).${format.name}'));
  }
  switch (format) {
    case ExportFormat.txt:
      await file.writeAsString('${preview.title}\n\n$body');
    case ExportFormat.md:
      await file.writeAsString('# ${preview.title}\n\n$body\n');
    case ExportFormat.docx:
      await file.writeAsBytes(docxBytes(preview.title, body));
    case ExportFormat.pdf:
      final font = pw.Font.ttf(await rootBundle.load('assets/NotoSansSC.ttf'));
      final document = pw.Document();
      document.addPage(
        pw.MultiPage(
          pageFormat: PdfPageFormat.a4,
          maxPages: 2000,
          margin: const pw.EdgeInsets.all(36),
          theme: pw.ThemeData.withFont(base: font, bold: font),
          build: (_) => [
            pw.Text(
              preview.title,
              style: pw.TextStyle(fontSize: 18, fontWeight: pw.FontWeight.bold),
            ),
            pw.SizedBox(height: 14),
            ...body
                .split('\n')
                .expand((line) {
                  final characters = line.runes.toList();
                  if (characters.isEmpty) return [''];
                  return [
                    for (var i = 0; i < characters.length; i += 1200)
                      String.fromCharCodes(
                        characters.sublist(
                          i,
                          (i + 1200).clamp(0, characters.length),
                        ),
                      ),
                  ];
                })
                .map(
                  (line) => pw.Padding(
                    padding: const pw.EdgeInsets.only(bottom: 5),
                    child: pw.Text(
                      line,
                      style: const pw.TextStyle(fontSize: 10),
                    ),
                  ),
                ),
          ],
        ),
      );
      await file.writeAsBytes(await document.save());
  }
  return file;
}
