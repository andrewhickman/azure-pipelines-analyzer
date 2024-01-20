use serde::{Deserialize, Serialize};

use crate::syntax::Span;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    span: Span,
    severity: Severity,
    message: String,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum Severity {
    Hint,
    Information,
    Warning,
    Error,
}

impl Diagnostic {
    pub fn new(span: Span, severity: Severity, message: impl ToString) -> Self {
        Diagnostic {
            span,
            severity,
            message: message.to_string(),
        }
    }
}
