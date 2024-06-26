//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::{
    config::MAX_SYSCALL_NUM,
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str, MapPermission, VirtAddr},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus, get_taskinfo,check_maparea,
        add_maparea, remove_maparea, take_current_task, set_current,

    },
    timer::get_time_us,
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

/// Exit the current task
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// Yield the CPU to the scheduler
pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// Get the pid of current task
pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

/// Fork the current task
pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

/// exec
pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    // 以只读的方式在内核中打开应用文件并获取它对应的 OSInode
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        // 通过 OSInode::read_all 将该文件的数据全部读到一个向量 all_data 中
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let us = get_time_us();
    let ts = translated_refmut(current_user_token(), _ts);
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
    trace!(
        "kernel:pid[{}] sys_task_info NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let ti = translated_refmut(current_user_token(), _ti);
    let task_ref = get_taskinfo();
    *ti = TaskInfo {
        status: TaskStatus::Running,
        syscall_times: task_ref.syscall_times,
        time: (get_time_us() - task_ref.time) / 1000,
    };
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let _path = translated_str(token, _path);
    // 以只读的方式在内核中打开应用文件并获取它对应的 OSInode
    if let Some(app_inode) = open_file(_path.as_str(), OpenFlags::RDONLY) {
        // 通过 OSInode::read_all 将该文件的数据全部读到一个向量 all_data 中
        let all_data = app_inode.read_all();
        let current_task = current_task().unwrap();
        let new_task = current_task.spawn(all_data.as_slice());
        let new_pid = new_task.pid.0;
        let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
        trap_cx.x[10] = 0;
        add_task(new_task);
        return new_pid as isize;
    } else {
        return -1;
    }

}

/// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio < 2 {
        return -1;
    }
    let task = take_current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.pro_lev = _prio as usize;
    drop(inner);
    set_current(task);
    _prio
}
