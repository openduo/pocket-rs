// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
//! DuoDuo room protocol and state machines, without a platform runtime.
pub mod api;
pub mod audio;
pub mod error;
pub mod live;
pub mod pairing;
pub mod retry;
pub mod session;

pub use error::{Error, Result};
pub const CLIENT_API_MAJOR: u64 = 2;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
