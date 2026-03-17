use serde::Serialize;

// ─── Exit codes ───

pub const EXIT_OK: i32 = 0;
pub const EXIT_INPUT: i32 = 1; // parse + validation errors
pub const EXIT_IO: i32 = 2; // file I/O errors
pub const EXIT_PROCESS: i32 = 3; // render + export errors
pub const EXIT_INTERNAL: i32 = 4; // unexpected errors

// ─── Success responses ───

#[derive(Serialize)]
pub struct OkResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

impl OkResponse {
    pub fn with_path(path: &str) -> Self {
        Self {
            status: "ok",
            path: Some(path.to_string()),
            width: None,
            height: None,
            warnings: vec![],
        }
    }

    pub fn with_render(path: &str, width: u32, height: u32) -> Self {
        Self {
            status: "ok",
            path: Some(path.to_string()),
            width: Some(width),
            height: Some(height),
            warnings: vec![],
        }
    }
}

// ─── Validation responses ───

#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ValidationIssue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

#[derive(Serialize, Debug)]
pub struct ValidationIssue {
    pub path: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Warning {
    pub path: String,
    pub code: String,
    pub message: String,
}

// ─── Error responses ───

#[derive(Serialize)]
pub struct ErrorResponse {
    pub status: &'static str,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ValidationIssue>,
}

impl ErrorResponse {
    pub fn new(code: &str, phase: &str, message: &str) -> Self {
        Self {
            status: "error",
            code: code.to_string(),
            phase: Some(phase.to_string()),
            message: message.to_string(),
            suggestion: None,
            errors: vec![],
        }
    }

    pub fn validation(errors: Vec<ValidationIssue>) -> Self {
        Self {
            status: "error",
            code: "VALIDATION_FAILED".to_string(),
            phase: Some("validate".to_string()),
            message: format!("{} validation error(s)", errors.len()),
            suggestion: None,
            errors,
        }
    }
}

// ─── Review responses ───

#[derive(Serialize)]
pub struct ReviewResponse {
    pub status: &'static str,
    pub context: serde_json::Value,
    pub summary: ode_review::result::ReviewSummary,
    pub issues: Vec<ode_review::result::ReviewIssue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_rules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

// ─── Guide responses ───

#[derive(Serialize)]
pub struct GuideContentResponse {
    pub status: &'static str,
    pub format: &'static str,
    pub content: String,
}

#[derive(Serialize)]
pub struct GuideListResponse {
    pub status: &'static str,
    pub layers: Vec<GuideLayerInfo>,
}

#[derive(Serialize)]
pub struct GuideLayerInfo {
    pub id: String,
    pub name: String,
    pub contexts: Vec<String>,
}

// ─── Print helpers ───

pub fn print_json<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string(value).unwrap_or_else(|e| {
        format!(r#"{{"status":"error","code":"INTERNAL","message":"JSON serialization failed: {e}"}}"#)
    }));
}
