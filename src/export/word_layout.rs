use super::text_layout::{self, Kind};
use anyhow::{Context, Result};
use docx_rs::*;
use std::io::Cursor;

fn fonts() -> RunFonts {
    RunFonts::new()
        .ascii("Arial")
        .hi_ansi("Arial")
        .east_asia("Microsoft YaHei")
}

fn run(text: &str, size: usize, gray: &str) -> Run {
    Run::new()
        .add_text(text)
        .size(size)
        .color(gray)
        .fonts(fonts())
}

fn inline(mut paragraph: Paragraph, text: &str, size: usize, color: &str) -> Paragraph {
    let mut text = text;
    while !text.is_empty() {
        let next = ["**", "`"]
            .iter()
            .filter_map(|m| text.find(m).map(|i| (i, *m)))
            .min_by_key(|v| v.0);
        if let Some((start, marker)) = next {
            if let Some(end) = text[start + marker.len()..]
                .find(marker)
                .map(|i| start + marker.len() + i)
            {
                paragraph = paragraph.add_run(run(&text[..start], size, color));
                let content = &text[start + marker.len()..end];
                paragraph = paragraph.add_run(if marker == "**" {
                    run(content, size, color).bold()
                } else {
                    run(content, size, color).fonts(
                        RunFonts::new()
                            .ascii("Consolas")
                            .east_asia("Microsoft YaHei"),
                    )
                });
                text = &text[end + marker.len()..];
                continue;
            }
        }
        paragraph = paragraph.add_run(run(text, size, color));
        break;
    }
    paragraph
}

pub(super) fn bytes(title_input: &str, body: &str) -> Result<Vec<u8>> {
    let title = text_layout::title(title_input, body);
    let spacing = |before, after| {
        LineSpacing::new()
            .before(before)
            .after(after)
            .line_rule(LineSpacingType::Auto)
            .line(330)
    };
    let mut document = Docx::new()
        .page_size(11906, 16838)
        .page_margin(
            PageMargin::new()
                .top(1361)
                .bottom(1361)
                .left(1247)
                .right(1247)
                .footer(680),
        )
        .default_fonts(fonts())
        .default_size(22)
        .add_style(
            Style::new("Normal", StyleType::Paragraph)
                .name("Normal")
                .fonts(fonts())
                .size(22)
                .color("222222")
                .line_spacing(spacing(0, 120)),
        )
        .add_style(
            Style::new("Title", StyleType::Paragraph)
                .name("Title")
                .fonts(fonts())
                .size(44)
                .bold()
                .color("000000")
                .line_spacing(spacing(0, 280)),
        )
        .add_style(
            Style::new("Heading1", StyleType::Paragraph)
                .name("heading 1")
                .fonts(fonts())
                .size(28)
                .bold()
                .color("000000")
                .outline_lvl(0)
                .line_spacing(spacing(240, 100)),
        )
        .add_style(
            Style::new("Heading2", StyleType::Paragraph)
                .name("heading 2")
                .fonts(fonts())
                .size(25)
                .bold()
                .color("000000")
                .outline_lvl(1)
                .line_spacing(spacing(200, 100)),
        )
        .add_paragraph(
            Paragraph::new()
                .style("Title")
                .keep_next(true)
                .keep_lines(true)
                .add_run(run(title, 44, "000000").bold()),
        )
        .footer(
            Footer::new().add_paragraph(
                Paragraph::new()
                    .align(AlignmentType::Center)
                    .add_run(run("拾文  ·  ", 16, "666666"))
                    .add_page_num(PageNum::new()),
            ),
        );
    let blocks = text_layout::blocks(body, title);
    let mut index = 0;
    while index < blocks.len() {
        let block = &blocks[index];
        let mut paragraph = Paragraph::new()
            .style("Normal")
            .widow_control(true)
            .line_spacing(spacing(0, 120));
        match block.kind {
            Kind::Divider => {
                index += 1;
                continue;
            }
            Kind::Heading(level) => {
                paragraph = paragraph
                    .style(if level <= 2 { "Heading1" } else { "Heading2" })
                    .outline_lvl(level.saturating_sub(2))
                    .keep_next(true)
                    .keep_lines(true)
                    .line_spacing(spacing(240, 100));
                paragraph = inline(
                    paragraph,
                    &block.text,
                    if level <= 2 { 28 } else { 25 },
                    "000000",
                );
            }
            Kind::Metadata => {
                paragraph = paragraph
                    .keep_next(index + 1 < blocks.len())
                    .line_spacing(spacing(0, 100));
                if let Some((label, value)) = text_layout::metadata(&block.text) {
                    paragraph = paragraph.add_run(run(&format!("{label}："), 18, "666666"));
                    if value.starts_with("https://") || value.starts_with("http://") {
                        paragraph = paragraph.add_hyperlink(
                            Hyperlink::new(value, HyperlinkType::External)
                                .add_run(run(value, 18, "526477").underline("single")),
                        );
                    } else {
                        paragraph = inline(paragraph, value, 18, "666666");
                    }
                } else {
                    paragraph = inline(paragraph, &block.text, 18, "666666");
                }
            }
            Kind::Quote => {
                let mut quote = block.text.trim_start_matches('>').trim().to_string();
                while blocks.get(index + 1).is_some_and(|b| b.kind == Kind::Quote) {
                    index += 1;
                    let text = blocks[index].text.trim_start_matches('>').trim();
                    if quote
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_ascii_alphanumeric())
                        && text
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_alphanumeric())
                    {
                        quote.push(' ');
                    }
                    quote.push_str(text);
                }
                paragraph = inline(
                    paragraph
                        .indent(Some(320), None, Some(160), None)
                        .line_spacing(spacing(100, 160)),
                    &quote,
                    21,
                    "555555",
                );
            }
            Kind::List => {
                paragraph = inline(
                    paragraph
                        .indent(Some(300), Some(SpecialIndentType::Hanging(240)), None, None)
                        .line_spacing(spacing(0, 60)),
                    &block.text,
                    22,
                    "222222",
                );
            }
            Kind::Verbatim if block.text.starts_with("```") || block.text.starts_with("~~~") => {
                let lines: Vec<_> = block.text.lines().collect();
                let last = if lines
                    .last()
                    .is_some_and(|s| s.trim().starts_with("```") || s.trim().starts_with("~~~"))
                {
                    lines.len() - 1
                } else {
                    lines.len()
                };
                for (n, line) in lines[1..last].iter().enumerate() {
                    if n > 0 {
                        paragraph =
                            paragraph.add_run(Run::new().add_break(BreakType::TextWrapping));
                    }
                    paragraph = paragraph.add_run(
                        run(line, 19, "333333").fonts(
                            RunFonts::new()
                                .ascii("Consolas")
                                .east_asia("Microsoft YaHei"),
                        ),
                    );
                }
            }
            _ => {
                paragraph = inline(paragraph, &block.text, 22, "222222");
            }
        }
        document = document.add_paragraph(paragraph);
        index += 1;
    }
    let mut buffer = Cursor::new(Vec::new());
    document
        .build()
        .pack(&mut buffer)
        .context("生成 Word 文件失败")?;
    Ok(buffer.into_inner())
}
