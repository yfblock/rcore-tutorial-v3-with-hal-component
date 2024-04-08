use super::TaskControlBlock;
use super::{fetch_task, TaskStatus};
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use arch::pagetable::PageTable;
use arch::{kernel_page_table, KContext, context_switch_pt};
use lazy_static::*;

pub struct Processor {
    current: Option<Arc<TaskControlBlock>>,
    idle_task_cx: KContext,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: KContext::blank(),
        }
    }
    fn get_idle_task_cx_ptr(&mut self) -> *mut KContext {
        &mut self.idle_task_cx as *mut _
    }
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const KContext;
            task_inner.task_status = TaskStatus::Running;
            // task_inner.memory_set.activate();
            let token = task_inner.memory_set.token();
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            // unsafe {
            //     __switch(idle_task_cx_ptr, next_task_cx_ptr);
            // }
            // token.change();
            // unsafe { arch::context_switch(idle_task_cx_ptr, next_task_cx_ptr) }
            unsafe { context_switch_pt(idle_task_cx_ptr, next_task_cx_ptr, token) }
        }
    }
}

pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

pub fn current_user_token() -> PageTable {
    let task = current_task().unwrap();
    let token = task.inner_exclusive_access().get_user_token();
    token
}

pub fn schedule(switched_task_cx_ptr: *mut KContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    // switch_to_kernel_page_table();
    drop(processor);
    // unsafe {
    //     __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    // }
    unsafe {
        context_switch_pt(switched_task_cx_ptr, idle_task_cx_ptr, kernel_page_table());
    }
}
