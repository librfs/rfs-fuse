// src/fs.rs
// SPDX-License-Identifier: AGPL-3.0
// Copyright (c) 2025 Canmi

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use librfs::{list_directory, model::Entry};
use nix::unistd::{Gid, Uid};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::runtime::Handle;

const TTL: Duration = Duration::from_secs(1);
const ROOT_INODE: u64 = 1;

// The RfsFuse struct now holds state for inode mapping.
pub struct RfsFuse {
    pool_root: String,
    tokio_handle: Handle,
    // In-memory mapping to track inodes.
    inodes: HashMap<u64, PathBuf>,
    paths: HashMap<PathBuf, u64>,
    next_inode: u64,
}

impl RfsFuse {
    // Constructor to create a new FUSE instance for a specific pool.
    pub fn new(pool_root: String) -> Self {
        let mut inodes = HashMap::new();
        let mut paths = HashMap::new();
        let root_path = PathBuf::from("/");

        // Initialize the root directory.
        inodes.insert(ROOT_INODE, root_path.clone());
        paths.insert(root_path, ROOT_INODE);

        Self {
            pool_root,
            tokio_handle: Handle::current(),
            inodes,
            paths,
            // Start assigning new inodes from 2 onwards.
            next_inode: ROOT_INODE + 1,
        }
    }

    // Helper to get or create an inode for a given path.
    fn get_or_create_inode(&mut self, path: &Path) -> u64 {
        if let Some(&ino) = self.paths.get(path) {
            return ino;
        }
        let new_ino = self.next_inode;
        self.next_inode += 1;
        self.paths.insert(path.to_path_buf(), new_ino);
        self.inodes.insert(new_ino, path.to_path_buf());
        new_ino
    }

    // Helper to build FileAttr from librfs Entry.
    fn entry_to_attr(&self, ino: u64, entry: &Entry) -> FileAttr {
        let (kind, size, modified_at) = match entry {
            Entry::File(f) => (FileType::RegularFile, f.size, f.modified_at),
            Entry::Directory(d) => (FileType::Directory, d.size, d.modified_at),
        };

        FileAttr {
            ino,
            size,
            blocks: (size + 511) / 512, // Calculate blocks based on size
            atime: SystemTime::now(), // Use current time for atime for simplicity
            mtime: modified_at.into(),
            ctime: modified_at.into(),
            crtime: modified_at.into(),
            kind,
            perm: if kind == FileType::Directory { 0o755 } else { 0o644 },
            nlink: 1,
            uid: Uid::current().as_raw(),
            gid: Gid::current().as_raw(),
            rdev: 0,
            flags: 0,
            blksize: 512,
        }
    }
}

impl Filesystem for RfsFuse {
    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let path = match self.inodes.get(&ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Handle root directory separately.
        if ino == ROOT_INODE {
            let attr = FileAttr {
                ino: ROOT_INODE,
                size: 4096, // Typical size for a directory
                blocks: 8,
                atime: SystemTime::now(),
                mtime: SystemTime::now(),
                ctime: SystemTime::now(),
                crtime: SystemTime::now(),
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2, // '.' and '..'
                uid: Uid::current().as_raw(),
                gid: Gid::current().as_raw(),
                rdev: 0,
                flags: 0,
                blksize: 512,
            };
            reply.attr(&TTL, &attr);
            return;
        }

        // For other files/dirs, find their entry in the parent listing.
        let parent_path = path.parent().unwrap_or_else(|| Path::new("/"));
        let file_name = path.file_name().unwrap_or_default();

        let listing_result = self.tokio_handle.block_on(
            list_directory(&self.pool_root, parent_path.to_str().unwrap_or("/"))
        );

        match listing_result {
            Ok(listing) => {
                if let Some(entry) = listing.get(file_name.to_str().unwrap()) {
                    let attr = self.entry_to_attr(ino, entry);
                    reply.attr(&TTL, &attr);
                } else {
                    reply.error(libc::ENOENT);
                }
            }
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let parent_path = match self.inodes.get(&parent) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let listing_result = self.tokio_handle.block_on(
            list_directory(&self.pool_root, parent_path.to_str().unwrap_or("/"))
        );

        match listing_result {
            Ok(listing) => {
                if let Some(entry) = listing.get(name.to_str().unwrap()) {
                    let child_path = parent_path.join(name);
                    let ino = self.get_or_create_inode(&child_path);
                    let attr = self.entry_to_attr(ino, entry);
                    reply.entry(&TTL, &attr, 0);
                } else {
                    reply.error(libc::ENOENT);
                }
            }
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let path = match self.inodes.get(&ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if offset == 0 {
            let _ = reply.add(ino, 0, FileType::Directory, ".");
            let parent_ino = if ino == ROOT_INODE {
                ROOT_INODE
            } else {
                let parent_path = path.parent().unwrap_or_else(|| Path::new("/"));
                self.get_or_create_inode(parent_path)
            };
            let _ = reply.add(parent_ino, 1, FileType::Directory, "..");

            let listing_result = self.tokio_handle.block_on(
                list_directory(&self.pool_root, path.to_str().unwrap_or("/"))
            );

            match listing_result {
                Ok(listing) => {
                    for (i, (name, entry)) in listing.iter().enumerate() {
                        let child_path = path.join(name);
                        let child_ino = self.get_or_create_inode(&child_path);
                        let kind = match entry {
                            Entry::File(_) => FileType::RegularFile,
                            Entry::Directory(_) => FileType::Directory,
                        };
                        if reply.add(child_ino, i as i64 + 2, kind, name) {
                            break;
                        }
                    }
                }
                Err(_) => {
                    reply.error(libc::EIO);
                    return;
                }
            }
        }
        reply.ok();
    }

    fn open(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        // TODO: Implement file opening based on path.
        reply.error(libc::ENOENT);
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        // TODO: Implement file reading based on path.
        reply.error(libc::ENOENT);
    }
}
