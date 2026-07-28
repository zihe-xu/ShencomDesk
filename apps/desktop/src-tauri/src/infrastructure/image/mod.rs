use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use image::{codecs::jpeg::JpegEncoder, ImageFormat};
use oxipng::Options;

use crate::application::image_service::{
    ImageProcessResult, ImageProcessStatus, ImageProcessor, ImageServiceError,
    ImageServiceErrorKind,
};

#[derive(Debug, Default)]
pub struct LocalImageProcessor;

impl ImageProcessor for LocalImageProcessor {
    fn process(
        &self,
        input: &Path,
        output: &Path,
        quality: u8,
    ) -> Result<ImageProcessResult, ImageServiceError> {
        let source = fs::read(input).map_err(|error| {
            ImageServiceError::new(
                ImageServiceErrorKind::Operation,
                format!("failed to read input image: {error}"),
            )
        })?;
        let extension = input
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| {
                ImageServiceError::new(
                    ImageServiceErrorKind::Unsupported,
                    "image extension is missing or invalid",
                )
            })?;

        let compressed = match extension.as_str() {
            "jpg" | "jpeg" => compress_jpeg(&source, quality)?,
            "png" => compress_png(&source)?,
            _ => {
                return Err(ImageServiceError::new(
                    ImageServiceErrorKind::Unsupported,
                    format!("unsupported image extension: {extension}"),
                ));
            }
        };

        let (status, output_bytes) = if compressed.len() < source.len() {
            (ImageProcessStatus::Completed, compressed.as_slice())
        } else {
            (ImageProcessStatus::Skipped, source.as_slice())
        };
        write_new_file(output, output_bytes)?;

        Ok(ImageProcessResult {
            status,
            original_bytes: source.len() as u64,
            output_bytes: output_bytes.len() as u64,
        })
    }
}

fn compress_jpeg(source: &[u8], quality: u8) -> Result<Vec<u8>, ImageServiceError> {
    let decoded =
        image::load_from_memory_with_format(source, ImageFormat::Jpeg).map_err(|error| {
            ImageServiceError::new(
                ImageServiceErrorKind::Decoding,
                format!("failed to decode JPEG: {error}"),
            )
        })?;
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, quality)
        .encode_image(&decoded)
        .map_err(|error| {
            ImageServiceError::new(
                ImageServiceErrorKind::Encoding,
                format!("failed to encode JPEG: {error}"),
            )
        })?;
    Ok(output)
}

fn compress_png(source: &[u8]) -> Result<Vec<u8>, ImageServiceError> {
    image::load_from_memory_with_format(source, ImageFormat::Png).map_err(|error| {
        ImageServiceError::new(
            ImageServiceErrorKind::Decoding,
            format!("failed to decode PNG: {error}"),
        )
    })?;
    oxipng::optimize_from_memory(source, &Options::from_preset(2)).map_err(|error| {
        ImageServiceError::new(
            ImageServiceErrorKind::Encoding,
            format!("failed to optimize PNG: {error}"),
        )
    })
}

fn write_new_file(output: &Path, bytes: &[u8]) -> Result<(), ImageServiceError> {
    let parent = output.parent().ok_or_else(|| {
        ImageServiceError::new(
            ImageServiceErrorKind::Output,
            "output path has no parent directory",
        )
    })?;
    let metadata = fs::metadata(parent).map_err(|error| {
        ImageServiceError::new(
            ImageServiceErrorKind::Output,
            format!("failed to read output directory: {error}"),
        )
    })?;
    if !metadata.is_dir() {
        return Err(ImageServiceError::new(
            ImageServiceErrorKind::Output,
            "output parent is not a directory",
        ));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| {
            ImageServiceError::new(
                ImageServiceErrorKind::Output,
                format!("failed to create output file: {error}"),
            )
        })?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = fs::remove_file(output);
        return Err(ImageServiceError::new(
            ImageServiceErrorKind::Output,
            format!("failed to write output file: {error}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use image::{DynamicImage, Rgb, RgbImage};

    use super::*;

    fn temp_directory(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "shendesk-image-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("temporary directory should be created");
        path
    }

    fn jpeg_fixture(path: &Path, quality: u8) {
        let mut image = RgbImage::new(256, 256);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgb([
                ((x * 31 + y * 17) % 256) as u8,
                ((x * 13 + y * 47) % 256) as u8,
                ((x * 7 + y * 29) % 256) as u8,
            ]);
        }
        let mut file = fs::File::create(path).expect("fixture should be created");
        JpegEncoder::new_with_quality(&mut file, quality)
            .encode_image(&DynamicImage::ImageRgb8(image))
            .expect("fixture should encode");
    }

    #[test]
    fn jpeg_output_is_smaller_and_decodable() {
        let root = temp_directory("jpeg");
        let input = root.join("input.jpg");
        let output_dir = root.join("output");
        let output = output_dir.join("input.jpg");
        fs::create_dir(&output_dir).expect("output directory should exist");
        jpeg_fixture(&input, 100);

        let result = LocalImageProcessor
            .process(&input, &output, 30)
            .expect("JPEG should compress");
        let decoded = image::open(&output).expect("output should decode");

        assert_eq!(result.status, ImageProcessStatus::Completed);
        assert!(result.output_bytes < result.original_bytes);
        assert_eq!(decoded.width(), 256);
        assert_eq!(decoded.height(), 256);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn png_output_is_not_larger_and_remains_decodable() {
        let root = temp_directory("png");
        let input = root.join("input.png");
        let output_dir = root.join("output");
        let output = output_dir.join("input.png");
        fs::create_dir(&output_dir).expect("output directory should exist");
        let image = RgbImage::from_fn(128, 128, |x, y| {
            Rgb([
                ((x * 11 + y * 3) % 256) as u8,
                ((x * 5 + y * 17) % 256) as u8,
                ((x * 19 + y * 7) % 256) as u8,
            ])
        });
        DynamicImage::ImageRgb8(image)
            .save_with_format(&input, ImageFormat::Png)
            .expect("fixture should encode");

        let result = LocalImageProcessor
            .process(&input, &output, 75)
            .expect("PNG should process");
        let decoded = image::open(&output).expect("output should decode");

        assert!(result.output_bytes <= result.original_bytes);
        assert_eq!(decoded.width(), 128);
        assert_eq!(decoded.height(), 128);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skipped_images_are_copied_byte_for_byte() {
        let root = temp_directory("skipped");
        let input = root.join("input.jpg");
        let output_dir = root.join("output");
        let output = output_dir.join("input.jpg");
        fs::create_dir(&output_dir).expect("output directory should exist");
        jpeg_fixture(&input, 1);
        let original = fs::read(&input).expect("fixture should be readable");

        let result = LocalImageProcessor
            .process(&input, &output, 100)
            .expect("JPEG should process");

        assert_eq!(result.status, ImageProcessStatus::Skipped);
        assert_eq!(
            fs::read(&output).expect("output should be readable"),
            original
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn never_overwrites_an_existing_output() {
        let root = temp_directory("collision");
        let input = root.join("input.jpg");
        let output_dir = root.join("output");
        let output = output_dir.join("input.jpg");
        fs::create_dir(&output_dir).expect("output directory should exist");
        jpeg_fixture(&input, 100);
        fs::write(&output, b"existing").expect("existing output should be created");

        let error = LocalImageProcessor
            .process(&input, &output, 30)
            .expect_err("existing output should fail");

        assert_eq!(error.kind(), ImageServiceErrorKind::Output);
        assert_eq!(
            fs::read(&output).expect("existing output should remain"),
            b"existing"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unsupported_extensions() {
        let root = temp_directory("unsupported");
        let input = root.join("input.gif");
        let output = root.join("output.gif");
        fs::write(&input, b"GIF89a").expect("fixture should be created");

        let error = LocalImageProcessor
            .process(&input, &output, 75)
            .expect_err("GIF should be rejected");

        assert_eq!(error.kind(), ImageServiceErrorKind::Unsupported);
        let _ = fs::remove_dir_all(root);
    }
}
