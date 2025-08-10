// src/error.rs
// SPDX-License-Identifier: AGPL-3.0
// Copyright (c) 2025 Canmi

use thiserror::Error;

#[derive(Error, Debug)]
pub enum FuseError {
    #[error("FUSE I/O or signal error: {0}")]
    Io(#[from] std::io::Error),
}
