// Copyright 2026 openduo and OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct RedialBackoff {
    pub failures: u32,
}
impl RedialBackoff {
    pub fn failed(&mut self) -> u64 {
        self.failures = self.failures.saturating_add(1);
        (1000u64 << (self.failures.saturating_sub(1)).min(16)).min(30_000)
    }
    pub fn sustained(&self) -> bool {
        self.failures >= 6
    }
    pub fn reset(&mut self) {
        self.failures = 0;
    }
}

/// The host supplies elapsed monotonic milliseconds, never a wall-clock deadline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceRetry {
    started_ms: u64,
    budget_ms: u64,
    failures: u32,
}
impl VoiceRetry {
    pub fn new(now_ms: u64, remaining_budget_ms: u64) -> Self {
        Self {
            started_ms: now_ms,
            budget_ms: remaining_budget_ms.min(25_000),
            failures: 0,
        }
    }
    pub fn remaining(&self, now_ms: u64) -> u64 {
        self.budget_ms
            .saturating_sub(now_ms.saturating_sub(self.started_ms))
    }
    pub fn failed(&mut self, now_ms: u64) -> Option<u64> {
        let remaining = self.remaining(now_ms);
        if remaining == 0 {
            return None;
        }
        let delay = (500u64 << self.failures.min(16)).min(4000).min(remaining);
        self.failures = self.failures.saturating_add(1);
        Some(delay)
    }
}
