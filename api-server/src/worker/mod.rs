// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod context;
pub mod jobs;
mod runner;

pub use runner::*;

use tari_engine_types::template_lib_models::ResourceAddress;

#[derive(Debug, Clone)]
pub enum JobResult {
    Completed { result: serde_json::Value },
    RetryIn { duration: std::time::Duration },
    WaitForBalance { resource: ResourceAddress, amount: u64 },
}
