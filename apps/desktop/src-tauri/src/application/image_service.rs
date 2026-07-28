use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::domain::image::{
    CompressImagesRequest, CompressImagesResult, CompressionProgress, CompressionStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageServiceErrorKind {
    Validation,
    Decoding,
    Encoding,
    Unsupported,
    Output,
    Operation,
}

#[derive(Debug, Clone)]
pub struct ImageServiceError {
    kind: ImageServiceErrorKind,
    message: String,
}

impl ImageServiceError {
    pub fn new(kind: ImageServiceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ImageServiceErrorKind::Validation, message)
    }

    pub fn kind(&self) -> ImageServiceErrorKind {
        self.kind
    }

    pub fn safe_message(&self) -> &'static str {
        match self.kind {
            ImageServiceErrorKind::Validation => "提交的数据无效，请检查后重试。",
            ImageServiceErrorKind::Decoding => "无法读取图片内容。",
            ImageServiceErrorKind::Encoding => "图片压缩失败。",
            ImageServiceErrorKind::Unsupported => "仅支持 PNG 和 JPEG 图片。",
            ImageServiceErrorKind::Output => "无法写入输出文件，文件可能已存在或目录不可写。",
            ImageServiceErrorKind::Operation => "图片处理失败，请重试。",
        }
    }
}

impl fmt::Display for ImageServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ImageServiceError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProcessStatus {
    Completed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageProcessResult {
    pub status: ImageProcessStatus,
    pub original_bytes: u64,
    pub output_bytes: u64,
}

pub trait ImageProcessor: Send + Sync {
    fn process(
        &self,
        input: &Path,
        output: &Path,
        quality: u8,
    ) -> Result<ImageProcessResult, ImageServiceError>;
}

pub type CompressionProgressHandler = Arc<dyn Fn(CompressionProgress) + Send + Sync + 'static>;

#[derive(Clone)]
pub struct ImageService {
    processor: Arc<dyn ImageProcessor>,
}

impl fmt::Debug for ImageService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageService")
            .finish_non_exhaustive()
    }
}

impl ImageService {
    pub fn new(processor: Arc<dyn ImageProcessor>) -> Self {
        Self { processor }
    }

    pub fn compress_images(
        &self,
        request: CompressImagesRequest,
        on_progress: CompressionProgressHandler,
    ) -> Result<CompressImagesResult, ImageServiceError> {
        let (items, output_dir) = validate_request(&request)?;
        let total = items.len();
        let mut result = CompressImagesResult {
            total,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            total_original_bytes: 0,
            total_compressed_bytes: 0,
            output_dir: output_dir.to_string_lossy().into_owned(),
        };

        for (position, input) in items.iter().enumerate() {
            let index = position + 1;
            let file_name = input
                .file_name()
                .expect("validated image path should have a file name")
                .to_string_lossy()
                .into_owned();
            on_progress(CompressionProgress {
                index,
                total,
                file_name: file_name.clone(),
                status: CompressionStatus::Processing,
                original_bytes: 0,
                compressed_bytes: 0,
                error: None,
            });

            let output = output_dir.join(&file_name);
            match self.processor.process(input, &output, request.quality) {
                Ok(processed) => {
                    let status = match processed.status {
                        ImageProcessStatus::Completed => {
                            result.succeeded += 1;
                            CompressionStatus::Completed
                        }
                        ImageProcessStatus::Skipped => {
                            result.skipped += 1;
                            CompressionStatus::Skipped
                        }
                    };
                    result.total_original_bytes = result
                        .total_original_bytes
                        .saturating_add(processed.original_bytes);
                    result.total_compressed_bytes = result
                        .total_compressed_bytes
                        .saturating_add(processed.output_bytes);
                    on_progress(CompressionProgress {
                        index,
                        total,
                        file_name,
                        status,
                        original_bytes: processed.original_bytes,
                        compressed_bytes: processed.output_bytes,
                        error: None,
                    });
                }
                Err(error) => {
                    tracing::error!(
                        input = %input.display(),
                        output = %output.display(),
                        error = %error,
                        "image processing failed"
                    );
                    result.failed += 1;
                    on_progress(CompressionProgress {
                        index,
                        total,
                        file_name,
                        status: CompressionStatus::Failed,
                        original_bytes: 0,
                        compressed_bytes: 0,
                        error: Some(error.safe_message().to_owned()),
                    });
                }
            }
        }

        Ok(result)
    }
}

fn validate_request(
    request: &CompressImagesRequest,
) -> Result<(Vec<PathBuf>, PathBuf), ImageServiceError> {
    if request.items.is_empty() {
        return Err(ImageServiceError::validation(
            "at least one image is required",
        ));
    }
    if !(1..=100).contains(&request.quality) {
        return Err(ImageServiceError::validation(
            "quality must be between 1 and 100",
        ));
    }

    let output_dir = validate_absolute_path(&request.output_dir, "output directory")?;
    let items = request
        .items
        .iter()
        .map(|item| {
            let path = validate_absolute_path(item, "image path")?;
            if path.file_name().is_none() {
                return Err(ImageServiceError::validation(
                    "image path must include a file name",
                ));
            }
            Ok(path)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok((items, output_dir))
}

fn validate_absolute_path(value: &str, label: &str) -> Result<PathBuf, ImageServiceError> {
    let value = value.trim();
    let path = PathBuf::from(value);
    if value.is_empty() || !path.is_absolute() {
        return Err(ImageServiceError::validation(format!(
            "{label} must be an absolute path"
        )));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Default)]
    struct RecordingProcessor {
        calls: Mutex<Vec<(PathBuf, PathBuf, u8)>>,
    }

    impl ImageProcessor for RecordingProcessor {
        fn process(
            &self,
            input: &Path,
            output: &Path,
            quality: u8,
        ) -> Result<ImageProcessResult, ImageServiceError> {
            self.calls.lock().expect("calls lock").push((
                input.to_owned(),
                output.to_owned(),
                quality,
            ));
            match input.file_name().and_then(|name| name.to_str()) {
                Some("completed.jpg") => Ok(ImageProcessResult {
                    status: ImageProcessStatus::Completed,
                    original_bytes: 100,
                    output_bytes: 60,
                }),
                Some("skipped.png") => Ok(ImageProcessResult {
                    status: ImageProcessStatus::Skipped,
                    original_bytes: 40,
                    output_bytes: 40,
                }),
                _ => Err(ImageServiceError::new(
                    ImageServiceErrorKind::Decoding,
                    "private decoder detail",
                )),
            }
        }
    }

    #[test]
    fn processes_in_order_and_continues_after_item_failures() {
        let processor = Arc::new(RecordingProcessor::default());
        let service = ImageService::new(processor.clone());
        let progress = Arc::new(Mutex::new(Vec::new()));
        let received = progress.clone();
        let result = service
            .compress_images(
                CompressImagesRequest {
                    items: vec![
                        "/tmp/completed.jpg".to_owned(),
                        "/tmp/failed.jpg".to_owned(),
                        "/tmp/skipped.png".to_owned(),
                    ],
                    output_dir: "/tmp/output".to_owned(),
                    quality: 75,
                },
                Arc::new(move |event| {
                    received.lock().expect("progress lock").push(event);
                }),
            )
            .expect("valid batch should complete");

        assert_eq!(result.total, 3);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.total_original_bytes, 140);
        assert_eq!(result.total_compressed_bytes, 100);

        let events = progress.lock().expect("progress lock");
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].index, 1);
        assert_eq!(events[1].status, CompressionStatus::Completed);
        assert_eq!(events[3].status, CompressionStatus::Failed);
        assert_eq!(events[3].error.as_deref(), Some("无法读取图片内容。"));
        assert_eq!(events[5].status, CompressionStatus::Skipped);
        assert!(!events[3]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("private"));

        let calls = processor.calls.lock().expect("calls lock");
        assert_eq!(calls[0].2, 75);
        assert_eq!(calls[0].1, PathBuf::from("/tmp/output/completed.jpg"));
    }

    #[test]
    fn validates_the_batch_before_processing() {
        let service = ImageService::new(Arc::new(RecordingProcessor::default()));
        let progress: CompressionProgressHandler = Arc::new(|_| {});

        for request in [
            CompressImagesRequest {
                items: Vec::new(),
                output_dir: "/tmp/output".to_owned(),
                quality: 75,
            },
            CompressImagesRequest {
                items: vec!["relative.jpg".to_owned()],
                output_dir: "/tmp/output".to_owned(),
                quality: 75,
            },
            CompressImagesRequest {
                items: vec!["/tmp/photo.jpg".to_owned()],
                output_dir: "/tmp/output".to_owned(),
                quality: 0,
            },
        ] {
            let error = service
                .compress_images(request, progress.clone())
                .expect_err("invalid request should fail");
            assert_eq!(error.kind(), ImageServiceErrorKind::Validation);
        }
    }
}
