// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::wallet::Wallet;

pub struct JobContext<TStore> {
    pub store: TStore,
    pub wallet: Wallet,
}
