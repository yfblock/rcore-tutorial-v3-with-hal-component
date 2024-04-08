#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

use crate::{
    syscall::syscall,
    task::{
        check_signals_error_of_current, current_add_signal, exit_current_and_run_next,
        handle_signals, suspend_current_and_run_next, SignalFlags,
    },
};
use arch::{TrapFrame, TrapFrameArgs, TrapType};
use arch::api::ArchInterface;
use arch::addr::PhysPage;
use crate_interface::impl_interface;
use fdt::node::FdtNode;
use log::warn;

use arch::TrapType::*;
extern crate alloc;

#[macro_use]
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;

#[macro_use]
mod console;
mod config;
mod drivers;
mod fs;
mod lang_items;
mod logging;
mod mm;
mod sync;
mod syscall;
mod task;

pub struct ArchInterfaceImpl;

#[impl_interface]
impl ArchInterface for ArchInterfaceImpl {
    /// Init allocator
    fn init_allocator() {
        mm::init_heap();
    }
    /// kernel interrupt
    fn kernel_interrupt(ctx: &mut TrapFrame, trap_type: TrapType) {
        // println!("trap_type @ {:x?} {:#x?}", trap_type, ctx);
        match trap_type {
            Breakpoint => return,
            UserEnvCall => {
                // jump to next instruction anyway
                ctx.syscall_ok();
                let args = ctx.args();
                // get system call return value
                // info!("syscall: {}", ctx[TrapFrameArgs::SYSCALL]);

                let result = syscall(ctx[TrapFrameArgs::SYSCALL], [args[0], args[1], args[2]]);
                // cx is changed during sys_exec, so we have to call it again
                ctx[TrapFrameArgs::RET] = result as usize;
            }
            StorePageFault(_paddr) | LoadPageFault(_paddr) | InstructionPageFault(_paddr) => {
                /*
                println!(
                    "[kernel] {:?} in application, bad addr = {:#x}, bad instruction = {:#x}, kernel killed it.",
                    scause.cause(),
                    stval,
                    current_trap_cx().sepc,
                );
                */
                current_add_signal(SignalFlags::SIGSEGV);
            }
            IllegalInstruction(_) => {
                current_add_signal(SignalFlags::SIGILL);
            }
            Time => {
                suspend_current_and_run_next();
            }
            _ => {
                warn!("unsuspended trap type: {:?}", trap_type);
            }
        }
        // handle signals (handle the sent signal)
        // println!("[K] trap_handler:: handle_signals");
        handle_signals();

        // check error signals (if error then exit)
        if let Some((errno, msg)) = check_signals_error_of_current() {
            println!("[kernel] {}", msg);
            exit_current_and_run_next(errno);
        }
    }
    /// init log
    fn init_logging() {
        logging::init(Some("trace"));
        println!("init logging");
    }
    /// add a memory region
    fn add_memory_region(start: usize, end: usize) {
        println!("init memory region {:#x} - {:#x}", start, end);
        mm::init_frame_allocator(start, end);
    }
    /// kernel main function, entry point.
    fn main(hartid: usize) {
        if hartid != 0 {
            return;
        }
        println!("[kernel] Hello, world!");
        arch::init_interrupt();
        // enable_irq();
        fs::list_apps();
        task::add_initproc();
        task::run_tasks();
        panic!("Unreachable in rust_main!");
    }
    /// Alloc a persistent memory page.
    fn frame_alloc_persist() -> PhysPage {
        // PhysPage::new(frame_alloc())
        mm::frame_alloc_persist().expect("can't find memory page")
    }
    /// Unalloc a persistent memory page
    fn frame_unalloc(ppn: PhysPage) {
        mm::frame_dealloc(ppn)
    }
    /// Preprare drivers.
    fn prepare_drivers() {
        println!("prepare drivers");
    }
    /// Try to add device through FdtNode
    fn try_to_add_device(_fdt_node: &FdtNode) {}
}
