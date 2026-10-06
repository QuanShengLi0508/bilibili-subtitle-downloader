use anyhow::{Context, Result};
use printpdf::{Color, Line, Mm, PdfDocument, Point, Rgb};
use std::io::Cursor;

const FONT: &[u8] = include_bytes!("../../assets/fonts/ShiwenSans-Regular.ttf");
const LEFT: f32 = 22.0;
const WIDTH: f32 = 166.0;
const BOTTOM: f32 = 29.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Body,
    Heading,
    Metadata,
    Quote,
    Divider,
}
#[derive(Debug)]
struct Block {
    text: String,
    kind: Kind,
}
#[derive(Debug)]
struct Text {
    text: String,
    x: f32,
    y: f32,
    size: f32,
    gray: f32,
    quote: bool,
}

fn join_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        if out
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii() && !c.is_whitespace())
            && text
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            out.push(' ');
        }
        out.push_str(text);
    }
    plain_inline(out)
}
fn plain_inline(mut text: String) -> String {
    // html2text emits these delimiters for emphasis and inline code.
    for marker in ["**", "`"] {
        let mut start = 0;
        while let Some(open) = text[start..].find(marker).map(|p| start + p) {
            let Some(close) = text[open + marker.len()..]
                .find(marker)
                .map(|p| open + marker.len() + p)
            else {
                break;
            };
            text.replace_range(close..close + marker.len(), "");
            text.replace_range(open..open + marker.len(), "");
            start = close - marker.len();
        }
    }
    text
}
fn blocks(body: &str, title: &str) -> Vec<Block> {
    let mut result = Vec::new();
    let mut pending = Vec::new();
    let mut pending_kind = Kind::Body;
    let flush = |pending: &mut Vec<String>, kind, result: &mut Vec<Block>| {
        if !pending.is_empty() {
            result.push(Block {
                text: join_lines(pending),
                kind,
            });
            pending.clear();
        }
    };
    for (index, line) in body.lines().enumerate() {
        let text = line.trim();
        if index == 0 && text.trim_end_matches(['?', '？']) == title.trim_end_matches(['?', '？'])
        {
            continue;
        }
        if text.is_empty() {
            flush(&mut pending, pending_kind, &mut result);
            continue;
        }
        let kind = if matches!(text, "---" | "***" | "___") {
            Kind::Divider
        } else if text.starts_with("回答 ") && text.contains('|')
            || text.starts_with("# ")
            || text.starts_with("## ")
            || text.starts_with("### ")
        {
            Kind::Heading
        } else if ["链接：", "链接:", "来源：", "来源:", "作者：", "作者:"]
            .iter()
            .any(|p| text.starts_with(p))
            || text.starts_with("已获取 ") && text.contains("回答")
            || text.starts_with("共 ") && text.contains("回答") && text.contains("导出")
        {
            Kind::Metadata
        } else if text.starts_with('>') {
            Kind::Quote
        } else {
            Kind::Body
        };
        let list = text.starts_with("- ") || text.starts_with("* ") || text.starts_with("• ");
        if kind != pending_kind
            || matches!(kind, Kind::Heading | Kind::Metadata | Kind::Divider)
            || list
        {
            flush(&mut pending, pending_kind, &mut result);
        }
        if matches!(kind, Kind::Heading | Kind::Metadata | Kind::Divider) {
            result.push(Block {
                text: plain_inline(text.trim_start_matches('#').trim().into()),
                kind,
            });
        } else {
            pending_kind = kind;
            pending.push(if kind == Kind::Quote {
                text.trim_start_matches('>').trim().into()
            } else {
                text.into()
            });
            if list {
                flush(&mut pending, kind, &mut result);
            }
        }
    }
    flush(&mut pending, pending_kind, &mut result);
    result
}

fn width(text: &str, face: &ttf_parser::Face<'_>, size: f32) -> f32 {
    text.chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_hor_advance(g))
                .unwrap_or(face.units_per_em()) as f32
                / face.units_per_em() as f32
                * size
                * 25.4
                / 72.0
        })
        .sum()
}
fn closes(c: char) -> bool {
    "，。！？；：、）】》〉”’％,.!?;:)]}%".contains(c)
}
fn opens(c: char) -> bool {
    "（【《〈“‘([{".contains(c)
}

pub(super) fn wrap(text: &str, face: &ttf_parser::Face<'_>, size: f32, max: f32) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
            word.push(c);
        } else {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            tokens.push(if c == '\t' { " ".into() } else { c.to_string() });
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    let mut result = Vec::new();
    let mut line = String::new();
    for token in tokens {
        let parts = if width(&token, face, size) > max {
            token.chars().map(|c| c.to_string()).collect::<Vec<_>>()
        } else {
            vec![token]
        };
        for token in parts {
            if line.is_empty() && token == " " {
                continue;
            }
            if width(&(line.clone() + &token), face, size) > max && !line.is_empty() {
                let mut carry = String::new();
                if line.chars().last().is_some_and(opens)
                    || token.chars().next().is_some_and(closes)
                {
                    if token.chars().next().is_some_and(closes)
                        && line
                            .chars()
                            .last()
                            .is_some_and(|c| c.is_ascii_alphanumeric())
                    {
                        while line
                            .chars()
                            .last()
                            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                        {
                            if let Some(last) = line.pop() {
                                carry.insert(0, last);
                            }
                        }
                    } else if let Some(last) = line.pop() {
                        carry.push(last);
                    }
                }
                if !line.trim().is_empty() {
                    result.push(line.trim_end().to_owned());
                }
                line = carry;
            }
            line.push_str(&token);
        }
    }
    if !line.trim().is_empty() {
        result.push(line.trim_end().to_owned());
    }
    result
}
fn clean_title(title: &str) -> &str {
    if let Some((name, range)) = title.rsplit_once("_回答") {
        if !range.is_empty() && range.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return name.trim_end_matches('_');
        }
    }
    title.trim()
}

fn layout(title: &str, body: &str, face: &ttf_parser::Face<'_>) -> Vec<Vec<Text>> {
    let mut pages = vec![Vec::new()];
    let mut y = 265.0;
    for line in wrap(title, face, 21.0, WIDTH) {
        if y < BOTTOM + 35.0 {
            pages.push(Vec::new());
            y = 265.0;
        }
        pages.last_mut().unwrap().push(Text {
            text: line,
            x: LEFT,
            y,
            size: 21.0,
            gray: 0.12,
            quote: false,
        });
        y -= 10.0;
    }
    y -= 5.5;
    let content = blocks(body, title);
    for (index, block) in content.iter().enumerate() {
        if block.kind == Kind::Divider {
            y -= 4.0;
            continue;
        }
        let (size, leading, before, after, x, gray) = match block.kind {
            Kind::Heading => (13.5, 7.2, 5.0, 2.5, LEFT, 0.14),
            Kind::Metadata => (8.5, 4.5, 0.0, 2.0, LEFT, 0.45),
            Kind::Quote => (10.5, 6.0, 2.0, 4.0, LEFT + 5.0, 0.36),
            _ => (11.0, 6.4, 0.0, 3.8, LEFT, 0.17),
        };
        let lines = wrap(&block.text, face, size, WIDTH - (x - LEFT));
        if lines.is_empty() {
            continue;
        }
        y -= before;
        let reserve =
            if matches!(block.kind, Kind::Heading | Kind::Metadata) && index + 1 < content.len() {
                leading * lines.len() as f32
                    + if block.kind == Kind::Heading {
                        28.0
                    } else {
                        17.0
                    }
            } else {
                leading * lines.len().min(2) as f32
            };
        if y - reserve < BOTTOM {
            pages.push(Vec::new());
            y = 269.0;
        }
        let mut offset = 0;
        while offset < lines.len() {
            let capacity = (((y - BOTTOM) / leading).floor() as usize + 1).max(1);
            let mut count = capacity.min(lines.len() - offset);
            if lines.len() - offset > count && lines.len() - offset - count == 1 && count > 2 {
                count -= 1;
            }
            for line in &lines[offset..offset + count] {
                pages.last_mut().unwrap().push(Text {
                    text: line.clone(),
                    x,
                    y,
                    size,
                    gray,
                    quote: block.kind == Kind::Quote,
                });
                y -= leading;
            }
            offset += count;
            if offset < lines.len() {
                pages.push(Vec::new());
                y = 269.0;
            }
        }
        y -= after;
    }
    pages
}

pub(super) fn bytes(title: &str, body: &str) -> Result<Vec<u8>> {
    let title = clean_title(title);
    let first = body.lines().next().unwrap_or_default().trim();
    let title = if first.trim_end_matches(['?', '？']) == title.trim_end_matches(['?', '？']) {
        first
    } else {
        title
    };
    let face = ttf_parser::Face::parse(FONT, 0).context("读取 PDF 中文字体失败")?;
    let pages = layout(title, body, &face);
    let (document, first_page, first_layer) = PdfDocument::new(title, Mm(210.0), Mm(297.0), "文字");
    let font = document.add_external_font(Cursor::new(FONT))?;
    for (index, page) in pages.iter().enumerate() {
        let (page_id, layer_id) = if index == 0 {
            (first_page, first_layer)
        } else {
            document.add_page(Mm(210.0), Mm(297.0), "文字")
        };
        let layer = document.get_page(page_id).get_layer(layer_id);
        let draw_rule = |x1, y1, x2, y2, gray| {
            layer.set_outline_color(Color::Rgb(Rgb::new(gray, gray, gray, None)));
            layer.set_outline_thickness(0.5);
            layer.add_line(Line {
                points: vec![
                    (Point::new(Mm(x1), Mm(y1)), false),
                    (Point::new(Mm(x2), Mm(y2)), false),
                ],
                is_closed: false,
            });
        };
        layer.set_fill_color(Color::Rgb(Rgb::new(0.55, 0.55, 0.55, None)));
        let header = if index == 0 {
            "拾文 · 文字存档".into()
        } else {
            wrap(title, &face, 8.0, WIDTH)
                .first()
                .cloned()
                .unwrap_or_default()
        };
        layer.use_text(header, 8.0, Mm(LEFT), Mm(281.0), &font);
        if index > 0 {
            draw_rule(LEFT, 277.0, LEFT + WIDTH, 277.0, 0.88);
        }
        for text in page {
            layer.set_fill_color(Color::Rgb(Rgb::new(text.gray, text.gray, text.gray, None)));
            layer.use_text(&text.text, text.size, Mm(text.x), Mm(text.y), &font);
            if text.quote {
                draw_rule(LEFT + 1.0, text.y - 1.5, LEFT + 1.0, text.y + 4.5, 0.82);
            }
        }
        draw_rule(LEFT, 22.0, LEFT + WIDTH, 22.0, 0.88);
        layer.set_fill_color(Color::Rgb(Rgb::new(0.55, 0.55, 0.55, None)));
        layer.use_text("拾文", 8.0, Mm(LEFT), Mm(15.0), &font);
        let page_number = format!("{} / {}", index + 1, pages.len());
        layer.use_text(
            &page_number,
            8.0,
            Mm(LEFT + WIDTH - width(&page_number, &face, 8.0)),
            Mm(15.0),
            &font,
        );
    }
    document.save_to_bytes().context("生成 PDF 文件失败")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_aliases_have_distinct_glyphs_for_copy_and_search() {
        let face = ttf_parser::Face::parse(FONT, 0).unwrap();
        for (text, radical) in [('一', '\u{2f00}'), ('文', '\u{2f42}'), ('用', '\u{2f64}')] {
            assert_ne!(face.glyph_index(text), face.glyph_index(radical));
        }
    }
    #[test]
    fn emphasis_delimiters_do_not_leak_into_pdf_text() {
        assert_eq!(
            plain_inline("**重要内容** 与 `code` 和 **第二段**".into()),
            "重要内容 与 code 和 第二段"
        );
    }
    #[test]
    fn reflows_words_and_keeps_closing_punctuation_off_line_starts() {
        let face = ttf_parser::Face::parse(FONT, 0).unwrap();
        let text = "中文排版需要避免，标点符号出现在行首。 English words should stay together.";
        let lines = wrap(text, &face, 11.0, 25.0);
        assert!(lines.iter().all(|s| !s.chars().next().is_some_and(closes)));
        assert!(lines.iter().any(|s| s.contains("English")));
        assert_eq!(lines.join("").replace(' ', ""), text.replace(' ', ""));
    }
    #[test]
    fn pagination_keeps_last_paragraph_and_headings_with_text() {
        let face = ttf_parser::Face::parse(FONT, 0).unwrap();
        let body = format!("回答 1 | 作者\n链接：https://www.zhihu.com/question/123/answer/456\n\n{}\n\n最终段落完整保留", "这是一段长中文正文，用于验证自动分页。\n\n".repeat(160));
        let pages = layout("测试标题", &body, &face);
        assert!(pages.len() > 2);
        assert!(pages.iter().flatten().all(|t| t.y >= BOTTOM));
        assert!(pages
            .last()
            .unwrap()
            .iter()
            .any(|t| t.text.contains("最终段落完整保留")));
        assert_eq!(clean_title("测试标题__回答1-22"), "测试标题");
    }
}
