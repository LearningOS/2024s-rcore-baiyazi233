//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus, add_maparea, remove_maparea, current_user_token, get_taskinfo, check_maparea 
    },
    timer::get_time_us,
    mm::{VirtAddr, MapPermission,translated_struct_ptr},
};

#[repr(C)]
#[derive(Debug)]
/// Time value
pub struct TimeVal {
    /// Second
    pub sec: usize,
    /// Microsecond
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    pub status: TaskStatus,
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    pub time: usize,
}

#[allow(dead_code)]
impl TaskInfo {
    /// Create a new TaskInfo
    pub fn new() -> Self {
        TaskInfo {
            status: TaskStatus::Running,
            syscall_times: [0; MAX_SYSCALL_NUM],
            time: get_time_us(),
        }
    }
}


/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let ts = translated_struct_ptr(current_user_token(), _ts);
    *ts = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    let ti = translated_struct_ptr(current_user_token(), _ti);
    let task_ref = get_taskinfo();
    *ti = TaskInfo {
        status: TaskStatus::Running,
        syscall_times: task_ref.syscall_times,
        time: (get_time_us() - task_ref.time) / 1000,
    };
    0
}

/// YOUR JOB: Implement mmap.
/// 将虚拟地址空间中的一段映射到物理地址空间中的一段
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    if _len == 0 {
        return 0;
    }
    if _port & !0x7 != 0 || _port & 0x7 == 0 {
        return -1;
    }
    let _end = _start + _len;
    let start_va = VirtAddr::from(_start);
    if !start_va.aligned() {
        debug!("unmap fail don't aligned");
        return -1;
    }
    let end_va = VirtAddr::from(_end);
    if check_maparea(start_va, end_va) {
        debug!("unmap fail conflict");
        return -1;
    }
    let mut map_perm = MapPermission::U;
    if 1 as usize & _port != 0 {
        map_perm |= MapPermission::R;
    }
    if 2 as usize & _port != 0 {
        map_perm |= MapPermission::W;
    }
    if 4 as usize & _port != 0 {
        map_perm |= MapPermission::X;
    }
    add_maparea(start_va, end_va, map_perm);
    0

}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    let _end = _start + _len;
    let start_va = VirtAddr::from(_start);
    if !start_va.aligned() {
        debug!("unmap fail don't aligned");
        return -1;
    }
    let end_va = VirtAddr::from(_end);
    remove_maparea(start_va, end_va)
    

}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
