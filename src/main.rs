// src/main.rs
// SPDX-License-Identifier: AGPL-3.0
// Copyright (c) 2025 Canmi

mod error;
mod fs;

use error::FuseError;
use fs::RfsFuse;
use fuser::{spawn_mount2, MountOption};
use rfs_ess::load_config;
use rfs_utils::{log, set_log_level, LogLevel};
use signal_hook::consts::{SIGINT, SIGTERM};
use std::ffi::{OsStr, OsString};
use std::process;

const CONFIG_PATH: &str = "/opt/rfs/rfsd/config.toml";

fn main() {
    // Load main config.
    let config = match load_config(CONFIG_PATH) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}. Exiting.", e);
            process::exit(1);
        }
    };

    // Set log level from the config.
    set_log_level(config.common.log_level);
    log(LogLevel::Info, "Logger initialized for rfs-fuse.");

    // Parse mountpoint from command-line arguments.
    let mountpoint: OsString = match std::env::args_os().nth(1) {
        Some(mp) => mp,
        None => {
            log(LogLevel::Error, "Usage: rfs <MOUNTPOINT>");
            process::exit(1);
        }
    };

    // Run the FUSE filesystem, handling potential errors.
    if let Err(e) = run_fuse(&mountpoint) {
        log(LogLevel::Error, &format!("Filesystem failed: {}", e));
        process::exit(1);
    }

    // These messages are printed after run_fuse returns successfully.
    log(LogLevel::Info, "mount has shut down. Cleaning up fuse resources.");
    log(LogLevel::Info, &format!("Successfully umount {:?}", mountpoint));
}

// A synchronous function to set up and run the FUSE filesystem.
fn run_fuse(mountpoint: &OsStr) -> Result<(), FuseError> {
    log(LogLevel::Info, &format!("Mounting filesystem at {:?}", mountpoint));

    let options = vec![
        MountOption::FSName("rfs".to_string()),
        MountOption::AutoUnmount,
    ];

    // Use spawn_mount2 to run the filesystem in the background and get a
    // session guard. This is the key change.
    let _session = spawn_mount2(RfsFuse, mountpoint, &options)?;
    log(LogLevel::Info, "Filesystem mounted.");
    let mut signals = signal_hook::iterator::Signals::new(&[SIGINT, SIGTERM])?;
    for sig in signals.forever() {
        println!(); // Add a newline for cleaner log output after ^C

        // Map signal number to a more descriptive message.
        match sig {
            SIGINT => log(LogLevel::Info, "Received Ctrl+C signal."),
            SIGTERM => log(LogLevel::Info, "Received terminate signal."),
            _ => log(LogLevel::Info, &format!("Received signal {}.", sig)),
        }
        log(LogLevel::Info, "Initiating graceful shutdown.");
        // Breaking the loop will cause the function to return.
        break;
    }

    // The '_session' guard goes out of scope here, and its Drop
    // implementation unmounts the filesystem.
    Ok(())
}
