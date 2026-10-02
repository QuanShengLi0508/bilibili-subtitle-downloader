use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TextFormat {
    #[default]
    Txt,
    Markdown,
    Word,
    Pdf,
}

impl TextFormat {
    pub const ALL: [Self; 4] = [Self::Txt, Self::Markdown, Self::Word, Self::Pdf];

    pub fn label(self) -> &'static str {
        match self {
            Self::Txt => "TXT",
            Self::Markdown => "Markdown",
            Self::Word => "Word (.docx)",
            Self::Pdf => "PDF",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Markdown => "md",
            Self::Word => "docx",
            Self::Pdf => "pdf",
        }
    }
}

pub fn convert(source: &Path, format: TextFormat) -> Result<PathBuf> {
    if format == TextFormat::Txt {
        return Ok(source.to_owned());
    }
    let content = std::fs::read_to_string(source).context("读取文字失败")?;
    let title = source.file_stem().unwrap_or_default().to_string_lossy();
    let body = content
        .strip_prefix(title.as_ref())
        .unwrap_or(&content)
        .trim_start();
    let destination = source.with_extension(format.extension());
    save(&destination, &title, body, format)?;
    Ok(destination)
}

pub fn save(path: &Path, title: &str, body: &str, format: TextFormat) -> Result<()> {
    let bytes = match format {
        TextFormat::Txt => format!("{title}\n\n{body}").into_bytes(),
        TextFormat::Markdown => format!("# {title}\n\n{body}\n").into_bytes(),
        TextFormat::Word => word_bytes(title, body)?,
        TextFormat::Pdf => pdf_bytes(title, body)?,
    };
    let parent = path.parent().context("输出目录不存在")?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .context("保存文件失败，文件可能正在被其他程序使用")?;
    Ok(())
}

fn word_bytes(title: &str, body: &str) -> Result<Vec<u8>> {
    use docx_rs::{Docx, Paragraph, Run, RunFonts};
    let fonts = RunFonts::new().ascii("Calibri").east_asia("微软雅黑");
    let mut document = Docx::new().add_paragraph(
        Paragraph::new().add_run(
            Run::new()
                .add_text(title)
                .bold()
                .size(36)
                .fonts(fonts.clone()),
        ),
    );
    for line in body.lines() {
        document = document.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(line).size(22).fonts(fonts.clone())),
        );
    }
    let mut buffer = Cursor::new(Vec::new());
    document
        .build()
        .pack(&mut buffer)
        .context("生成 Word 文件失败")?;
    Ok(buffer.into_inner())
}

fn pdf_bytes(title: &str, body: &str) -> Result<Vec<u8>> {
    use printpdf::{Mm, PdfDocument};
    let font_path = [
        r"C:\Windows\Fonts\simhei.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .into_iter()
    .find(|path| Path::new(path).is_file())
    .context("未找到 PDF 中文字体")?;
    let font_bytes = std::fs::read(font_path)?;
    let face = ttf_parser::Face::parse(&font_bytes, 0).context("读取 PDF 字体失败")?;
    let (document, mut page, mut layer) = PdfDocument::new(title, Mm(210.0), Mm(297.0), "文字");
    let font = document.add_external_font(Cursor::new(&font_bytes))?;
    let mut y = 276.0;
    for (text, size, spacing) in [(title, 18.0_f32, 9.0_f32), (body, 11.0, 6.0)] {
        for paragraph in text.lines() {
            for line in wrap_line(paragraph, &face, size, 170.0) {
                if y < 22.0 {
                    (page, layer) = document.add_page(Mm(210.0), Mm(297.0), "文字");
                    y = 276.0;
                }
                document.get_page(page).get_layer(layer).use_text(
                    &line,
                    size,
                    Mm(20.0),
                    Mm(y),
                    &font,
                );
                y -= spacing;
            }
        }
        y -= 4.0;
    }
    document.save_to_bytes().context("生成 PDF 文件失败")
}

fn wrap_line(text: &str, face: &ttf_parser::Face<'_>, size: f32, max_mm: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut width = 0.0;
    for character in text.chars() {
        let character = if character == '\t' { ' ' } else { character };
        let advance = face
            .glyph_index(character)
            .and_then(|glyph| face.glyph_hor_advance(glyph))
            .unwrap_or(face.units_per_em());
        let mm = advance as f32 / face.units_per_em() as f32 * size * 25.4 / 72.0;
        if width + mm > max_mm && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            width = 0.0;
        }
        line.push(character);
        width += mm;
    }
    lines.push(line);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_unicode_and_multiple_pages() {
        let dir = tempfile::tempdir().unwrap();
        let body = "中文段落，包含 English 和 <特殊符号>。\n\n".repeat(150);
        for format in TextFormat::ALL {
            let path = dir.path().join(format!("中文标题.{}", format.extension()));
            save(&path, "中文标题", &body, format).unwrap();
            let bytes = std::fs::read(path).unwrap();
            match format {
                TextFormat::Pdf => assert!(bytes.starts_with(b"%PDF-")),
                TextFormat::Word => assert!(bytes.starts_with(b"PK")),
                _ => assert!(String::from_utf8(bytes).unwrap().contains(&body)),
            }
        }
        if let Ok(path) = std::env::var("SHIWEN_EXPORT_SMOKE_DIR") {
            let dir = PathBuf::from(path);
            let title = "拾文中文导出验证";
            for format in TextFormat::ALL {
                save(
                    &dir.join(format!("sample.{}", format.extension())),
                    title,
                    &body,
                    format,
                )
                .unwrap();
            }
        }
    }
}
