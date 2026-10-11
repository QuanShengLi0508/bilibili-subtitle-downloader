//! Image-aware fallback for Official Accounts articles. The preferred Windows
//! path prints the live article WebView, which can retain its CSS; this fallback
//! keeps paragraphs and downloaded images in their original reading order.
use anyhow::{bail, Context, Result};
use image::{imageops::FilterType, DynamicImage, ImageReader, Limits};
use printpdf::{
    Color, ColorBits, ColorSpace, Image, ImageTransform, ImageXObject, Line, Mm, PdfDocument,
    Point, Px, Rgb,
};
use std::io::Cursor;
use std::path::{Path, PathBuf};

const FONT: &[u8] = include_bytes!("../../assets/fonts/ShiwenSans-Regular.ttf");
const LEFT: f32 = 22.0;
const WIDTH: f32 = 166.0;
const BOTTOM: f32 = 28.0;
const TOP: f32 = 268.0;

#[derive(Clone, Debug)]
enum Block {
    Text(String, TextKind),
    Image(String),
}

#[derive(Clone, Copy, Debug)]
enum TextKind {
    Body,
    Heading,
    Metadata,
}

#[derive(Debug)]
enum Item {
    Text {
        value: String,
        x: f32,
        y: f32,
        size: f32,
        gray: f32,
    },
    Image {
        path: PathBuf,
        crop_top: u32,
        crop_height: u32,
        source_width: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

/// Matches only the relative filenames created by `Article::export`.
fn image_marker(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("![配图")?;
    let (_, filename) = rest.split_once("](")?;
    let filename = filename.strip_suffix(')')?;
    let basename = filename.strip_prefix("配图")?;
    let (number, ext) = basename.split_once('.')?;
    (number.len() == 3
        && number.bytes().all(|b| b.is_ascii_digit())
        && matches!(ext, "jpg" | "png" | "gif" | "webp" | "avif"))
    .then_some(filename)
}

fn blocks(body: &str) -> Result<Vec<Block>> {
    let mut result = Vec::new();
    let mut paragraph = String::new();
    let flush = |result: &mut Vec<Block>, paragraph: &mut String| {
        if !paragraph.is_empty() {
            result.push(Block::Text(std::mem::take(paragraph), TextKind::Body));
        }
    };
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut result, &mut paragraph);
            continue;
        }
        if line.starts_with("![") {
            let filename = image_marker(line).context("公众号配图引用无效，PDF 未生成")?;
            flush(&mut result, &mut paragraph);
            result.push(Block::Image(filename.to_owned()));
            continue;
        }
        if line.starts_with('#') {
            flush(&mut result, &mut paragraph);
            result.push(Block::Text(
                line.trim_start_matches('#').trim().to_owned(),
                TextKind::Heading,
            ));
            continue;
        }
        if ["作者：", "发布时间：", "来源：", "内容范围："]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        {
            flush(&mut result, &mut paragraph);
            result.push(Block::Text(line.to_owned(), TextKind::Metadata));
            continue;
        }
        if !paragraph.is_empty() {
            // The WebView reader can produce soft line wraps within a paragraph.
            if paragraph
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_alphanumeric())
                && line
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric())
            {
                paragraph.push(' ');
            }
        }
        paragraph.push_str(line);
    }
    flush(&mut result, &mut paragraph);
    Ok(result)
}

fn read_dimensions(path: &Path) -> Result<(u32, u32)> {
    let size = std::fs::metadata(path)?.len();
    if size > 50_000_000 {
        bail!("公众号配图超过 50 MB，PDF 未生成");
    }
    let dimensions = ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()
        .context("公众号配图无法读取尺寸")?;
    if dimensions.0 == 0
        || dimensions.1 == 0
        || dimensions.0 > 12_000
        || dimensions.1 > 40_000
        || u64::from(dimensions.0) * u64::from(dimensions.1) > 40_000_000
    {
        bail!("公众号配图尺寸异常，PDF 未生成");
    }
    Ok(dimensions)
}

fn plan(title: &str, body: &str, images_dir: &Path) -> Result<Vec<Vec<Item>>> {
    let face = ttf_parser::Face::parse(FONT, 0).context("读取 PDF 中文字体失败")?;
    let mut pages = vec![Vec::new()];
    let mut y = TOP;
    for line in super::pdf_layout::wrap(title, &face, 20.0, WIDTH) {
        if y < BOTTOM + 30.0 {
            pages.push(Vec::new());
            y = TOP;
        }
        pages.last_mut().unwrap().push(Item::Text {
            value: line,
            x: LEFT,
            y,
            size: 20.0,
            gray: 0.12,
        });
        y -= 10.0;
    }
    y -= 7.0;
    for block in blocks(body)? {
        match block {
            Block::Text(value, kind) => {
                let (size, leading, after, gray) = match kind {
                    TextKind::Heading => (14.0, 7.8, 4.5, 0.12),
                    TextKind::Metadata => (8.5, 4.6, 1.2, 0.43),
                    TextKind::Body => (11.0, 6.5, 4.2, 0.17),
                };
                let lines = super::pdf_layout::wrap(&value, &face, size, WIDTH);
                if y - leading * (lines.len().min(2) as f32) < BOTTOM {
                    pages.push(Vec::new());
                    y = TOP;
                }
                for line in lines {
                    if y < BOTTOM + leading {
                        pages.push(Vec::new());
                        y = TOP;
                    }
                    pages.last_mut().unwrap().push(Item::Text {
                        value: line,
                        x: LEFT,
                        y,
                        size,
                        gray,
                    });
                    y -= leading;
                }
                y -= after;
            }
            Block::Image(filename) => {
                let path = images_dir.join(filename);
                if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
                    bail!("公众号配图不是普通文件，PDF 未生成");
                }
                let (source_width, source_height) = read_dimensions(&path)?;
                // Keep small illustrations at a sensible size and fit large images
                // within the article column without changing their aspect ratio.
                let display_width = (source_width as f32 * 25.4 / 144.0).min(WIDTH);
                let mm_per_pixel = display_width / source_width as f32;
                let x = LEFT + (WIDTH - display_width) / 2.0;
                let mut crop_top = 0;
                while crop_top < source_height {
                    if y < BOTTOM + 65.0 {
                        pages.push(Vec::new());
                        y = TOP;
                    }
                    let available = y - BOTTOM - 4.0;
                    let crop_height = ((available / mm_per_pixel).floor() as u32)
                        .max(1)
                        .min(source_height - crop_top);
                    let height = crop_height as f32 * mm_per_pixel;
                    pages.last_mut().unwrap().push(Item::Image {
                        path: path.clone(),
                        crop_top,
                        crop_height,
                        source_width,
                        x,
                        y: y - height,
                        width: display_width,
                        height,
                    });
                    y -= height + 5.0;
                    crop_top += crop_height;
                }
            }
        }
    }
    Ok(pages)
}

fn load_image(path: &Path) -> Result<DynamicImage> {
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(40_000);
    limits.max_alloc = Some(240_000_000);
    reader.limits(limits);
    reader.decode().with_context(|| {
        format!(
            "无法解码公众号配图 {}，PDF 未生成；请尝试从官方网页打印",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}

fn crop_image(image: &DynamicImage, top: u32, height: u32, source_width: u32) -> Result<Image> {
    if image.width() != source_width || top + height > image.height() {
        bail!("公众号配图在导出期间发生变化，PDF 未生成");
    }
    let cropped = image.crop_imm(0, top, source_width, height);
    let max_width = source_width.min(1_500);
    let resized = if source_width > max_width {
        cropped.resize(max_width, 2_400, FilterType::Lanczos3)
    } else {
        cropped
    };
    // PDF DeviceRGB has no alpha. Composite transparent PNG/WebP/GIF pixels on
    // white rather than turning transparent backgrounds black.
    let rgba = resized.to_rgba8();
    let mut rgb = Vec::with_capacity(rgba.width() as usize * rgba.height() as usize * 3);
    for pixel in rgba.pixels() {
        let alpha = u16::from(pixel[3]);
        for channel in &pixel.0[..3] {
            rgb.push(((u16::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8);
        }
    }
    Ok(Image::from(ImageXObject {
        width: Px(rgba.width() as usize),
        height: Px(rgba.height() as usize),
        color_space: ColorSpace::Rgb,
        bits_per_component: ColorBits::Bit8,
        interpolate: true,
        image_data: rgb,
        image_filter: None,
        smask: None,
        clipping_bbox: None,
    }))
}

/// Generates a PDF from the locally downloaded image package. Never fetches
/// remote content or follows file paths supplied by the webpage.
pub(crate) fn bytes(title: &str, body: &str, images_dir: &Path) -> Result<Vec<u8>> {
    let pages = plan(title, body, images_dir)?;
    let (document, first_page, first_layer) =
        PdfDocument::new(title, Mm(210.0), Mm(297.0), "公众号图文");
    let font = document.add_external_font(Cursor::new(FONT))?;
    let mut cached_image: Option<(PathBuf, DynamicImage)> = None;
    for (index, page) in pages.iter().enumerate() {
        let (page_id, layer_id) = if index == 0 {
            (first_page, first_layer)
        } else {
            document.add_page(Mm(210.0), Mm(297.0), "公众号图文")
        };
        let layer = document.get_page(page_id).get_layer(layer_id);
        layer.set_fill_color(Color::Rgb(Rgb::new(0.44, 0.48, 0.55, None)));
        layer.use_text("拾文 · 微信公众号图文存档", 8.0, Mm(LEFT), Mm(282.0), &font);
        for item in page {
            match item {
                Item::Text {
                    value,
                    x,
                    y,
                    size,
                    gray,
                } => {
                    // Release a decoded long image once its consecutive parts
                    // have ended; another article picture can be large too.
                    cached_image = None;
                    layer.set_fill_color(Color::Rgb(Rgb::new(*gray, *gray, *gray, None)));
                    layer.use_text(value, *size, Mm(*x), Mm(*y), &font);
                }
                Item::Image {
                    path,
                    crop_top,
                    crop_height,
                    source_width,
                    x,
                    y,
                    width,
                    height,
                } => {
                    if cached_image
                        .as_ref()
                        .is_none_or(|(cached_path, _)| cached_path != path)
                    {
                        cached_image = Some((path.clone(), load_image(path)?));
                    }
                    let image = crop_image(
                        &cached_image.as_ref().unwrap().1,
                        *crop_top,
                        *crop_height,
                        *source_width,
                    )?;
                    let pixel_width = image.image.width.0 as f32;
                    let pixel_height = image.image.height.0 as f32;
                    image.add_to_layer(
                        layer.clone(),
                        ImageTransform {
                            translate_x: Some(Mm(*x)),
                            translate_y: Some(Mm(*y)),
                            dpi: Some(72.0),
                            scale_x: Some(*width * 72.0 / (25.4 * pixel_width)),
                            scale_y: Some(*height * 72.0 / (25.4 * pixel_height)),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        layer.set_outline_color(Color::Rgb(Rgb::new(0.84, 0.87, 0.9, None)));
        layer.set_outline_thickness(0.4);
        layer.add_line(Line {
            points: vec![
                (Point::new(Mm(LEFT), Mm(22.0)), false),
                (Point::new(Mm(LEFT + WIDTH), Mm(22.0)), false),
            ],
            is_closed: false,
        });
        layer.set_fill_color(Color::Rgb(Rgb::new(0.50, 0.54, 0.60, None)));
        layer.use_text("拾文", 8.0, Mm(LEFT), Mm(15.0), &font);
        layer.use_text(
            format!("{} / {}", index + 1, pages.len()),
            8.0,
            Mm(LEFT + WIDTH - 18.0),
            Mm(15.0),
            &font,
        );
    }
    document.save_to_bytes().context("生成公众号图文 PDF 失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_interleaved_images_in_article_order_and_on_page() {
        let directory = tempfile::tempdir().unwrap();
        for filename in ["配图001.png", "配图002.png"] {
            let image = image::RgbImage::from_pixel(400, 240, image::Rgb([15, 80, 220]));
            image.save(directory.path().join(filename)).unwrap();
        }
        let body = "开头文字。\n\n![配图1](配图001.png)\n\n中间文字。\n\n![配图2](配图002.png)\n\n结尾文字。";
        let pages = plan("公众号标题", body, directory.path()).unwrap();
        let ordered = pages
            .iter()
            .flatten()
            .map(|item| match item {
                Item::Text { value, .. } => value.clone(),
                Item::Image { path, .. } => path.file_name().unwrap().to_string_lossy().into(),
            })
            .collect::<Vec<String>>();
        let joined = ordered.join("|");
        let positions = ["开头文字", "配图001", "中间文字", "配图002", "结尾文字"]
            .map(|part| joined.find(part).unwrap());
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        let pdf = bytes("公众号标题", body, directory.path()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        if let Ok(path) = std::env::var("SHIWEN_WECHAT_PDF_PREVIEW_PATH") {
            let path = Path::new(&path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, &pdf).unwrap();
        }
        let document = printpdf::lopdf::Document::load_mem(&pdf).unwrap();
        let embedded_images = document
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|value| value.as_name().ok())
                    == Some(b"Image".as_slice())
            })
            .count();
        assert_eq!(embedded_images, 2);
    }

    #[test]
    fn refuses_nonlocal_and_unexpected_image_references() {
        for input in [
            "![配图1](../private.png)",
            "![配图1](https://mmbiz.qpic.cn/a.png)",
            "![配图1](other.png)",
        ] {
            assert!(blocks(input).is_err(), "{input}");
        }
    }

    #[test]
    fn splits_tall_image_without_losing_following_paragraph() {
        let directory = tempfile::tempdir().unwrap();
        image::RgbImage::from_fn(400, 2400, |_, y| {
            if y < 800 {
                image::Rgb([230, 70, 70])
            } else if y < 1600 {
                image::Rgb([245, 210, 45])
            } else {
                image::Rgb([35, 130, 220])
            }
        })
        .save(directory.path().join("配图001.png"))
        .unwrap();
        let body = "图前。\n\n![配图1](配图001.png)\n\n图后的最后一段。";
        let pages = plan("长图", body, directory.path()).unwrap();
        assert!(pages.len() > 1);
        let image_parts = pages
            .iter()
            .flatten()
            .filter(|item| matches!(item, Item::Image { .. }))
            .count();
        assert!(image_parts > 1);
        assert!(pages.iter().flatten().all(|item| match item {
            Item::Text { y, .. } | Item::Image { y, .. } => *y >= BOTTOM - 0.1,
        }));
        assert!(pages.last().unwrap().iter().any(|item| {
            matches!(item, Item::Text { value, .. } if value.contains("图后的最后一段"))
        }));
        if let Ok(path) = std::env::var("SHIWEN_WECHAT_TALL_PDF_PREVIEW_PATH") {
            let path = Path::new(&path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes("长图", body, directory.path()).unwrap()).unwrap();
        }
    }
}
