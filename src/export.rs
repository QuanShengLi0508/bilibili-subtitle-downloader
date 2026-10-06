use anyhow::{Context, Result};
use std::io::Write;
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
        TextFormat::Markdown => text_layout::markdown(title, body).into_bytes(),
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
    word_layout::bytes(title, body)
}

mod pdf_layout;
mod text_layout;
mod word_layout;
fn pdf_bytes(title: &str, body: &str) -> Result<Vec<u8>> {
    pdf_layout::bytes(title, body)
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
                TextFormat::Markdown => {
                    let text = String::from_utf8(bytes).unwrap();
                    assert_eq!(text.matches("中文段落").count(), 150);
                    assert!(text.contains("&lt;特殊符号&gt;"));
                }
                TextFormat::Txt => assert!(String::from_utf8(bytes).unwrap().contains(&body)),
            }
        }
        if let Ok(source) = std::env::var("SHIWEN_TEXT_PREVIEW_SOURCE") {
            let source = PathBuf::from(source);
            let body = std::fs::read_to_string(&source).unwrap();
            let title = source.file_stem().unwrap().to_string_lossy();
            for format in [TextFormat::Markdown, TextFormat::Word] {
                save(
                    &PathBuf::from(format!(
                        "output/documents/知乎问答-优化排版-v1.5.4.{}",
                        format.extension()
                    )),
                    &title,
                    &body,
                    format,
                )
                .unwrap();
            }
        }
        if let Ok(source) = std::env::var("SHIWEN_PDF_PREVIEW_SOURCE") {
            let source = PathBuf::from(source);
            let body = std::fs::read_to_string(&source).unwrap();
            let title = source.file_stem().unwrap().to_string_lossy();
            save(
                &PathBuf::from("output/pdf/知乎问答-优化排版.pdf"),
                &title,
                &body,
                TextFormat::Pdf,
            )
            .unwrap();
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
