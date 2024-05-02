//! Task pid implementation.
//!
//! Assign PID to the process here. At the same time, the position of the application KernelStack
//! is determined according to the PID.

use crate::config::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE};
use crate::mm::{MapPermission, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;

/// 进程标识符分配器
pub struct RecycleAllocator {
    /// 当前可用的进程标识符
    current: usize,
    /// 已回收的进程标识符
    recycled: Vec<usize>,
}

impl RecycleAllocator {
    /// 创建一个新的进程标识符分配器
    pub fn new() -> Self {
        RecycleAllocator {
            current: 0,
            recycled: Vec::new(),
        }
    }

    /// 分配一个新的进程标识符
    pub fn alloc(&mut self) -> usize {
        if let Some(id) = self.recycled.pop() {
            id
        } else {
            self.current += 1;
            self.current - 1
        }
    }

    /// 释放一个进程标识符
    pub fn dealloc(&mut self, id: usize) {
        // 只回收已分配过的标识符
        assert!(id < self.current);
        // 不能重复回收
        assert!(
            !self.recycled.iter().any(|i| *i == id),
            "id {} has been deallocated!",
            id
        );
        self.recycled.push(id);
    }
}

lazy_static! {
    // 全局进程标识符分配器
    static ref PID_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
    static ref KSTACK_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
}

/// Abstract structure of PID
pub struct PidHandle(pub usize);

impl Drop for PidHandle {
    fn drop(&mut self) {
        //println!("drop pid {}", self.0);
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

/// Allocate a new PID
pub fn pid_alloc() -> PidHandle {
    PidHandle(PID_ALLOCATOR.exclusive_access().alloc())
}

/// Return (bottom, top) of a kernel stack in kernel space.
/// 根据传入的应用ID计算内核栈的起始（bottom）和结束（top）地址。
pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

/// Kernel stack for a process(task)
pub struct KernelStack(pub usize);

/// allocate a new kernel stack
/// 分配一个新的内核栈
pub fn kstack_alloc() -> KernelStack {
    // 从全局 KSTACK_ALLOCATOR 获取一个栈标识符
    let kstack_id = KSTACK_ALLOCATOR.exclusive_access().alloc();
    // 使用 kernel_stack_position 函数计算栈的位置
    let (kstack_bottom, kstack_top) = kernel_stack_position(kstack_id);
    // 将计算出的栈区域插入到内核的内存空间映射中，赋予读写权限
    KERNEL_SPACE.exclusive_access().insert_framed_area(
        kstack_bottom.into(),
        kstack_top.into(),
        MapPermission::R | MapPermission::W,
    );
    KernelStack(kstack_id)
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        // 计算栈底部地址
        let (kernel_stack_bottom, _) = kernel_stack_position(self.0);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        // 从内核空间映射中移除栈区域
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.into());
        // 释放栈标识符
        KSTACK_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

impl KernelStack {
    /// Push a variable of type T into the top of the KernelStack and return its raw pointer
    /// 将类型为 T 的变量推入栈顶，并返回该变量在栈上的原始指针
    #[allow(unused)]
    pub fn push_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        let kernel_stack_top = self.get_top();
        // 计算新变量的地址
        let ptr_mut = (kernel_stack_top - core::mem::size_of::<T>()) as *mut T;
        unsafe {
            *ptr_mut = value;
        }
        ptr_mut
    }

    /// Get the top of the KernelStack
    /// 获取内核栈的顶部地址
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.0);
        kernel_stack_top
    }
}
