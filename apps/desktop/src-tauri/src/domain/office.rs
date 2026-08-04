use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
}
