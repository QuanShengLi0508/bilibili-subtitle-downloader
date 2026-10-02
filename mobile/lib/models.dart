class TextEntry {
  const TextEntry(this.author, this.text, {this.url = ''});
  final String author;
  final String text;
  final String url;
}

class TextPreview {
  const TextPreview(
    this.title,
    this.entries, {
    this.isQuestion = false,
    this.srt = '',
  });
  final String title;
  final List<TextEntry> entries;
  final bool isQuestion;
  final String srt;
  String rangeBody(int first, int last) {
    if (first < 1 || last > entries.length || first > last) {
      throw ArgumentError('范围须在 1–${entries.length} 之间，起始不能大于结束');
    }
    final sections = <String>[];
    for (var i = first - 1; i < last; i++) {
      final e = entries[i];
      sections.add(
        isQuestion
            ? '回答 ${i + 1} | ${e.author}\n链接：${e.url}\n\n${e.text}'
            : e.text,
      );
    }
    return '${isQuestion ? '已获取 ${entries.length} 条回答 | 导出 $first–$last\n\n' : ''}${sections.join('\n\n---\n\n')}';
  }
}

class BiliVideo {
  const BiliVideo(this.title, this.bvid, this.pages);
  final String title;
  final String bvid;
  final List<Map<String, dynamic>> pages;
}
