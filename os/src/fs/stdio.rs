//!Stdin & Stdout
use super::File;
use crate::mm::UserBuffer;
use crate::sbi::console_getchar;
use crate::task::suspend_current_and_run_next;

/// stdin file for getting chars from console
pub struct Stdin;

/// stdout file for putting chars to console
pub struct Stdout;

impl File for Stdin {
    /// 标准输入文件 Stdin 是只读文件，只允许进程通过 read 从里面读入数据
    fn readable(&self) -> bool {
        true
    }
    fn writable(&self) -> bool {
        false
    }
    fn read(&self, mut user_buf: UserBuffer) -> usize {
        // 目前每次仅支持读入一个字符
        assert_eq!(user_buf.len(), 1);
        // busy loop
        let mut c: usize;
        loop {
            c = console_getchar();
            //表示没有可用字符，让出 CPU 给其他进程
            if c == 0 {
                suspend_current_and_run_next();
                continue;
            } else {
                break;
            }
        }
        // 将读入的字符写入到用户缓冲区中
        let ch = c as u8;
        unsafe {
            user_buf.buffers[0].as_mut_ptr().write_volatile(ch);
        }
        1
    }
    fn write(&self, _user_buf: UserBuffer) -> usize {
        panic!("Cannot write to stdin!");
    }
}

impl File for Stdout {
    fn readable(&self) -> bool {
        false
    }
    /// 标准输出文件 Stdout 是只写文件，只允许进程通过 write 写入到里面
    fn writable(&self) -> bool {
        true
    }
    fn read(&self, _user_buf: UserBuffer) -> usize {
        panic!("Cannot read from stdout!");
    }
    fn write(&self, user_buf: UserBuffer) -> usize {
        // 将用户缓冲区中的数据写入到控制台
        for buffer in user_buf.buffers.iter() {
            print!("{}", core::str::from_utf8(*buffer).unwrap());
        }
        user_buf.len()
    }
}
