use core::cell::LazyCell;

use super::TaskControlBlock;
use super::{fetch_task, TaskStatus};
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use lazy_static::*;
use lazyinit::LazyInit;
use log::*;
use polyhal::kcontext::{context_switch_pt, KContext};
use polyhal::pagetable::PageTable;
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
    trace!("os::task::processor::run_tasks");
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
            // from idel_task, switch to next task with next task's page table
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

static BOOT_PAGE_TABLE: LazyInit<PageTable> = LazyInit::new();

pub fn schedule(switched_task_cx_ptr: *mut KContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    // from switched task, switch to idle task with kernel page table
    // todo!("switch PageTable");
    unsafe {
        context_switch_pt(switched_task_cx_ptr, idle_task_cx_ptr, *BOOT_PAGE_TABLE);
    }
}

pub fn init_kernel_page() {
    BOOT_PAGE_TABLE.init_once(PageTable::current());
}
