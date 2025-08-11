// src/error.rs
// SPDX-License-Identifier: AGPL-3.0
// Copyright (c) 2025 Canmi

use thiserror::Error;

#[derive(Error, Debug)]
pub enum FuseError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Pool configuration error: {0}")]
    Pool(#[from] rfs_pool::PoolError),

    #[error("Metadata error: {0}")]
    Metadata(#[from] librfs::MetadataError),

    #[error("Mount configuration error: {0}")]
    MountConfig(String),
}
