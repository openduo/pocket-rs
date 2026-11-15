// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use serde::Serialize;

pub type Result<T> = std::result::Result<T, Error>;

/// Stable machine-readable codes. Applications localize these at their presentation boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct Error {
    pub code: String,
    pub detail: String,
}

impl Error {
    pub fn new(code: &str, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
        }
    }
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::new("invalid_input", detail)
    }
    pub fn protocol(detail: impl Into<String>) -> Self {
        Self::new("protocol_error", detail)
    }
}

impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::invalid("Malformed JSON or invalid field type")
    }
}
