use std::io::Cursor;

use exif::{In, Reader, Tag};
use image::{
    DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, codecs::webp::WebPEncoder,
    imageops::FilterType,
};

pub const OUTPUT_CONTENT_TYPE: &str = "image/webp";

pub struct ProcessedImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn process_upload(
    input: &[u8],
    max_width: u32,
    max_height: u32,
) -> Result<ProcessedImage, ImageProcessingError> {
    let format = image::guess_format(input).map_err(|_| ImageProcessingError::UnsupportedFormat)?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return Err(ImageProcessingError::UnsupportedFormat);
    }

    let image = image::load_from_memory_with_format(input, format)
        .map_err(|_| ImageProcessingError::InvalidImage)?;
    let oriented = apply_orientation(image, extract_orientation(input));
    let resized = resize_image(oriented, max_width, max_height);
    let width = resized.width();
    let height = resized.height();
    let bytes = encode_webp(resized)?;

    Ok(ProcessedImage {
        bytes,
        width,
        height,
    })
}

fn extract_orientation(input: &[u8]) -> u32 {
    let mut cursor = Cursor::new(input);
    Reader::new()
        .read_from_container(&mut cursor)
        .ok()
        .and_then(|exif| exif.get_field(Tag::Orientation, In::PRIMARY).cloned())
        .and_then(|field| field.value.get_uint(0))
        .unwrap_or(1)
}

fn apply_orientation(image: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate90().flipv(),
        8 => image.rotate270(),
        _ => image,
    }
}

fn resize_image(image: DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
    let src_width = image.width();
    let src_height = image.height();

    let scale = (max_width as f64 / src_width as f64)
        .min(max_height as f64 / src_height as f64)
        .min(1.0);

    let dst_width = ((src_width as f64 * scale).floor() as u32).max(1);
    let dst_height = ((src_height as f64 * scale).floor() as u32).max(1);

    if dst_width == src_width && dst_height == src_height {
        image
    } else {
        image.resize_exact(dst_width, dst_height, FilterType::Lanczos3)
    }
}

fn encode_webp(image: DynamicImage) -> Result<Vec<u8>, ImageProcessingError> {
    let rgba = image.to_rgba8();
    let mut output = Vec::new();
    let encoder = WebPEncoder::new_lossless(&mut output);
    encoder
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|_| ImageProcessingError::EncodeWebp)?;

    Ok(output)
}

#[derive(Debug)]
pub enum ImageProcessingError {
    UnsupportedFormat,
    InvalidImage,
    EncodeWebp,
}

#[cfg(test)]
mod tests {
    use image::{
        ExtendedColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage, codecs::png::PngEncoder,
    };

    use super::{OUTPUT_CONTENT_TYPE, process_upload};

    #[test]
    fn process_upload_resizes_png_and_outputs_webp() {
        let source = RgbaImage::from_pixel(10, 5, Rgba([255, 0, 0, 255]));
        let mut png_bytes = Vec::new();
        let encoder = PngEncoder::new(&mut png_bytes);
        encoder
            .write_image(source.as_raw(), 10, 5, ExtendedColorType::Rgba8)
            .unwrap();

        let processed = process_upload(&png_bytes, 4, 4).unwrap();
        assert_eq!(processed.width, 4);
        assert_eq!(processed.height, 2);
        assert!(!processed.bytes.is_empty());
        assert_eq!(OUTPUT_CONTENT_TYPE, "image/webp");

        let decoded =
            image::load_from_memory_with_format(&processed.bytes, ImageFormat::WebP).unwrap();
        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 2);
    }
}
