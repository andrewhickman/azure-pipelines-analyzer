use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {}

impl Diagnostic {
    pub(crate) fn invalid_encoding() -> Self {
        Diagnostic {}
    }
}
