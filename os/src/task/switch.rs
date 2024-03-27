use core::arch::global_asm;

use arch::TrapFrame;

global_asm!(include_str!("switch.S"));

extern "C" {
    pub fn __switch(current_task_cx_ptr: *mut TrapFrame, next_task_cx_ptr: *const TrapFrame);
}
