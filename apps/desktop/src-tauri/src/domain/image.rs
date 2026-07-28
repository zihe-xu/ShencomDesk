use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressImagesRequest {
    pub items: Vec<String>,
    pub output_dir: String,
    pub quality: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionStatus {
    Processing,
    Completed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionProgress {
    pub index: usize,
    pub total: usize,
    pub file_name: String,
    pub status: CompressionStatus,
    pub original_bytes: u64,
    pub compressed_bytes: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressImagesResult {
    pub total: usize,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub total_original_bytes: u64,
    pub total_compressed_bytes: u64,
    pub output_dir: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_camel_case_fields() {
        let request: CompressImagesRequest = serde_json::from_value(serde_json::json!({
            "items": ["/tmp/photo.jpg"],
            "outputDir": "/tmp/output",
            "quality": 75
        }))
        .expect("request should deserialize");

        assert_eq!(request.output_dir, "/tmp/output");
        assert_eq!(request.quality, 75);
    }
}
