// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[derive(Debug, Clone)]
pub enum PaymentProcessorEvent {
    TaskCreated { task_id: uuid::Uuid },
}
