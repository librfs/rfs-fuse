// src/main.rs
// SPDX-License-Identifier: AGPL-3.0
// Copyright (c) 2025 Canmi

mod error;
mod fs;

use error::FuseError;
use fs::RfsFuse;
use fuser::{spawn_mount2, MountOption};
use rfs_ess::load_config;
use rfs_pool::load_and_mount_pools;
use rfs_utils::{log, set_log_level, LogLevel};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::process;
use std::sync::Arc;
use tokio::task::JoinHandle;

const CONFIG_PATH: &str = "/opt/rfs/rfsd/config.toml";
const POOL_CONFIG_PATH: &str = "/opt/rfs/rfsd/pool.toml";

#[tokio::main]
async fn main() {
    // Load main config for logging.
    let config = match load_config(CONFIG_PATH) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}. Exiting.", e);
            process::exit(1);
        }
    };
    set_log_level(config.common.log_level);
    log(LogLevel::Info, "Logger initialized for rfs-fuse.");

    // Run the application and handle errors.
    if let Err(e) = run().await {
        log(LogLevel::Error, &format!("Filesystem failed: {}", e));
        process::exit(1);
    }
}

async fn run() -> Result<(), FuseError> {
    // Load pools and mount configurations.
    let (pools, mounts) = load_and_mount_pools(POOL_CONFIG_PATH).await?;
    if mounts.is_empty() {
        log(LogLevel::Warn, "No FUSE mounts defined in pool.toml. Exiting.");
        return Ok(());
    }

    // Create a quick lookup map from pool_id to pool_path.
    let pool_map: HashMap<u64, String> =
        pools.into_iter().map(|p| (p.pool_id, p.path)).collect();

    let mut join_handles = Vec::new();
    let mut session_guards = Vec::new();

    for mount_config in mounts {
        let pool_path = match pool_map.get(&mount_config.pool_id) {
            Some(path) => path.clone(),
            None => {
                return Err(FuseError::MountConfig(format!(
                    "Mount point '{}' references non-existent pool_id '{}'",
                    mount_config.mount_point, mount_config.pool_id
                )));
            }
        };

        let mount_point = Arc::new(mount_config.mount_point);
        let pool_root = Arc::new(pool_path);

        log(LogLevel::Info, &format!("Preparing to mount pool '{}' at '{}'", pool_root, mount_point));

        // Each FUSE instance needs to be spawned on a blocking-safe thread.
        let mount_point_clone = Arc::clone(&mount_point);
        let handle = tokio::task::spawn_blocking(move || {
            let fuse_fs = RfsFuse::new(pool_root.to_string());
            let options = vec![
                MountOption::FSName("rfs".to_string()),
                MountOption::AutoUnmount,
                MountOption::AllowRoot, // Often needed for system-wide mounts
            ];
            // This returns the session guard which must be kept alive.
            spawn_mount2(fuse_fs, mount_point_clone.as_str(), &options)
        });
        join_handles.push((mount_point, handle));
    }

    // Wait for the spawn_blocking tasks to finish setting up the mounts.
    for (mount_point, handle) in join_handles {
        match handle.await {
            Ok(Ok(session)) => {
                log(LogLevel::Info, &format!("Successfully mounted on {}", mount_point));
                session_guards.push(session);
            }
            Ok(Err(e)) => return Err(FuseError::Io(e)),
            Err(e) => return Err(FuseError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))),
        }
    }

    log(LogLevel::Info, "All filesystems mounted. Press Ctrl+C to unmount all.");

    // Wait for shutdown signal.
    tokio::signal::ctrl_c().await?;
    println!(); // Newline after ^C
    log(LogLevel::Info, "Received Ctrl+C signal.");
    log(LogLevel::Info, "Initiating graceful shutdown of all mounts.");

    // When this function returns, all `session_guards` will be dropped,
    // which unmounts each filesystem gracefully.
    Ok(())
}
