// src/fs.rs
// SPDX-License-Identifier: AGPL-3.0
// Copyright (c) 2025 Canmi

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use nix::unistd::{Gid, Uid};
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};

const TTL: Duration = Duration::from_secs(1);
const HELLO_TXT: &str = "Hello World from RFS!\n";
const HELLO_INODE: u64 = 2;

pub struct RfsFuse;

impl Filesystem for RfsFuse {
    fn getattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: Option<u64>,
        reply: ReplyAttr,
    ) {
        let attr = match ino {
            1 => FileAttr {
                ino: 1,
                size: 0,
                blocks: 0,
                atime: SystemTime::now(),
                mtime: SystemTime::now(),
                ctime: SystemTime::now(),
                crtime: SystemTime::now(),
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: Uid::current().as_raw(),
                gid: Gid::current().as_raw(),
                rdev: 0,
                flags: 0,
                blksize: 512,
            },
            HELLO_INODE => FileAttr {
                ino: HELLO_INODE,
                size: HELLO_TXT.len() as u64,
                blocks: 1,
                atime: SystemTime::now(),
                mtime: SystemTime::now(),
                ctime: SystemTime::now(),
                crtime: SystemTime::now(),
                kind: FileType::RegularFile,
                perm: 0o644,
                nlink: 1,
                uid: Uid::current().as_raw(),
                gid: Gid::current().as_raw(),
                rdev: 0,
                flags: 0,
                blksize: 512,
            },
            _ => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        reply.attr(&TTL, &attr);
    }

    fn lookup(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        name: &OsStr,
        reply: ReplyEntry,
    ) {
        if name.to_str() == Some("hello.txt") {
            let attr = FileAttr {
                ino: HELLO_INODE,
                size: HELLO_TXT.len() as u64,
                blocks: 1,
                atime: SystemTime::now(),
                mtime: SystemTime::now(),
                ctime: SystemTime::now(),
                crtime: SystemTime::now(),
                kind: FileType::RegularFile,
                perm: 0o644,
                nlink: 1,
                uid: Uid::current().as_raw(),
                gid: Gid::current().as_raw(),
                rdev: 0,
                flags: 0,
                blksize: 512,
            };
            reply.entry(&TTL, &attr, 0);
        } else {
            reply.error(libc::ENOENT);
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
        if ino != 1 {
            reply.error(libc::ENOENT);
            return;
        }

        if offset == 0 {
            // The `add` method returns a bool indicating if the buffer is full.
            // We explicitly ignore the result with `let _ =` to fix the warning.
            let _ = reply.add(1, 0, FileType::Directory, ".");
            let _ = reply.add(1, 1, FileType::Directory, "..");
            let _ = reply.add(HELLO_INODE, 2, FileType::RegularFile, "hello.txt");
        }
        reply.ok();
    }

    fn open(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        if ino == HELLO_INODE {
            reply.opened(0, 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        if ino == HELLO_INODE {
            let data = HELLO_TXT.as_bytes();
            let start = offset as usize;
            let end = std::cmp::min(start + size as usize, data.len());
            reply.data(&data[start..end]);
        } else {
            reply.error(libc::ENOENT);
        }
    }
}