//! File trait & inode(dir, file, pipe, stdin, stdout)

mod inode;
mod stdio;
use crate::mm::UserBuffer;

/// trait File for all file types
/// 接口在内存和I/O资源之间建立了数据交换的通道
/// UserBuffer 是我们在 mm 子模块中定义的应用地址空间中的一段缓冲区
pub trait File: Send + Sync + AnyConvertor{
    /// the file readable?
    fn readable(&self) -> bool;
    /// the file writable?
    fn writable(&self) -> bool;
    /// read from the file to buf, return the number of bytes read
    /// 从文件（即I/O资源）中读取数据放到缓冲区中，最多将缓冲区填满，并返回实际读取的字节数
    fn read(&self, buf: UserBuffer) -> usize;
    /// write to the file from buf, return the number of bytes written
    /// 将缓冲区中的数据写入文件，最多将缓冲区中的数据全部写入，并返回直接写入的字节数
    fn write(&self, buf: UserBuffer) -> usize;

    #[allow(unused_variables)]
    /// fstat
    fn fstat(&self, stat: &mut Stat) -> isize {
        -1
    }
}

use core::any::Any;

/// convert current type to &dyn Any
pub trait AnyConvertor {
    /// convert current type to &dyn Any
    fn as_any(&self) -> &dyn Any;
}

impl<T: 'static> AnyConvertor for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The stat of a inode
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    pub dev: u64,
    /// inode number
    pub ino: u64,
    /// file type and mode
    pub mode: StatMode,
    /// number of hard links
    pub nlink: u32,
    /// unused pad
    pub pad: [u64; 7],
}

bitflags! {
    /// The mode of a inode
    /// whether a directory or a file
    pub struct StatMode: u32 {
        /// null
        const NULL  = 0;
        /// directory
        const DIR   = 0o040000;
        /// ordinary regular file
        const FILE  = 0o100000;
    }
}

pub use inode::{list_apps, open_file, OSInode, OpenFlags, ROOT_INODE};
pub use stdio::{Stdin, Stdout};
