use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficeDocumentFormat {
    Word,
    Spreadsheet,
    Presentation,
}

impl OfficeDocumentFormat {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "docx" => Some(Self::Word),
            "xlsx" => Some(Self::Spreadsheet),
            "pptx" => Some(Self::Presentation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficeDocument {
    pub path: PathBuf,
    pub format: OfficeDocumentFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OfficeDocumentOperation {
    AddWordParagraph { text: String },
    SetSpreadsheetCell { cell: String, value: String },
    AddPresentationSlide { title: String },
    AddPresentationText { slide: u32, text: String },
}

impl OfficeDocumentOperation {
    pub fn supports_format(&self, format: OfficeDocumentFormat) -> bool {
        matches!(
            (self, format),
            (Self::AddWordParagraph { .. }, OfficeDocumentFormat::Word)
                | (
                    Self::SetSpreadsheetCell { .. },
                    OfficeDocumentFormat::Spreadsheet
                )
                | (
                    Self::AddPresentationSlide { .. } | Self::AddPresentationText { .. },
                    OfficeDocumentFormat::Presentation
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeInspection {
    pub format: OfficeDocumentFormat,
    pub structure: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficePreview {
    pub mime_type: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficeEngineState {
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeEngineStatus {
    pub state: OfficeEngineState,
    pub version: Option<String>,
}

impl OfficeEngineStatus {
    pub fn ready(version: impl Into<String>) -> Self {
        Self {
            state: OfficeEngineState::Ready,
            version: Some(version.into()),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            state: OfficeEngineState::Unavailable,
            version: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficeLifecycleOperation {
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeOperationResult {
    pub operation: OfficeLifecycleOperation,
    pub succeeded: bool,
    pub owns_session: bool,
}

impl OfficeOperationResult {
    pub fn succeeded(operation: OfficeLifecycleOperation) -> Self {
        Self {
            operation,
            succeeded: true,
            owns_session: false,
        }
    }

    pub fn opened(owns_session: bool) -> Self {
        Self {
            operation: OfficeLifecycleOperation::Open,
            succeeded: true,
            owns_session,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_supported_office_formats() {
        assert_eq!(
            OfficeDocumentFormat::from_extension("DOCX"),
            Some(OfficeDocumentFormat::Word)
        );
        assert_eq!(
            OfficeDocumentFormat::from_extension("xlsx"),
            Some(OfficeDocumentFormat::Spreadsheet)
        );
        assert_eq!(
            OfficeDocumentFormat::from_extension("pptx"),
            Some(OfficeDocumentFormat::Presentation)
        );
        assert_eq!(OfficeDocumentFormat::from_extension("pdf"), None);
    }

    #[test]
    fn operations_are_explicit_and_format_scoped() {
        let operation: OfficeDocumentOperation = serde_json::from_value(serde_json::json!({
            "type": "set_spreadsheet_cell",
            "cell": "A1",
            "value": "fixture"
        }))
        .expect("operation should deserialize");

        assert!(operation.supports_format(OfficeDocumentFormat::Spreadsheet));
        assert!(!operation.supports_format(OfficeDocumentFormat::Word));
        assert!(
            serde_json::from_value::<OfficeDocumentOperation>(serde_json::json!({
                "type": "set_spreadsheet_cell",
                "cell": "A1",
                "value": "fixture",
                "command": "raw-set"
            }))
            .is_err()
        );
    }
}
