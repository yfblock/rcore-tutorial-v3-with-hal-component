#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

use core::task::Context;

use arch::{ArchInterface, PhysPage, TrapFrame, TrapType};
use config::PAGE_SIZE;
use crate_interface::impl_interface;
use fdt::node::FdtNode;
use mm::{frame_alloc, frame_dealloc, PhysPageNum};

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
mod sbi;
mod sync;
mod syscall;
mod task;
mod timer;

pub struct ArchInterfaceImpl;

#[impl_interface]
impl ArchInterface for ArchInterfaceImpl {
    /// Init allocator
    fn init_allocator() {
        mm::init_heap();
    }
    /// kernel interrupt
    fn kernel_interrupt(ctx: &mut TrapFrame, trap_type: TrapType) {
        println!("trap_type @ {:x?} {:#x?}", trap_type, ctx);
    }
    /// init log
    fn init_logging() {
        logging::init(Some("debug"));
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
        // mm::init();
        // trap::init();
        timer::set_next_trigger();
        fs::list_apps();
        task::add_initproc();
        task::run_tasks();
        panic!("Unreachable in rust_main!");
    }
    /// Alloc a persistent memory page.
    fn frame_alloc_persist() -> PhysPage {
        PhysPage::new(frame_alloc().expect("can't alloc frame").ppn.0)
    }
    /// Unalloc a persistent memory page
    fn frame_unalloc(ppn: PhysPage) {
        frame_dealloc(PhysPageNum(ppn.to_addr() / PAGE_SIZE))
    }
    /// Preprare drivers.
    fn prepare_drivers() {
        println!("prepare drivers");
    }
    /// Try to add device through FdtNode
    fn try_to_add_device(fdt_node: &FdtNode) {}
}
