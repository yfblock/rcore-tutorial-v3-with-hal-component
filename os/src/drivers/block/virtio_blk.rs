use core::ptr::NonNull;

use super::BlockDevice;
use crate::mm::{frame_alloc, frame_dealloc, frames_alloc, FrameTracker};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;
use log::debug;
use polyhal::consts::VIRT_ADDR_START;
use polyhal::pagetable::PAGE_SIZE;
use polyhal::PhysAddr;
use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::{BufferDirection, Hal};

#[allow(unused)]
#[cfg(target_arch = "riscv64")]
const VIRTIO0: PhysAddr = polyhal::pa!(0x10001000);

#[cfg(target_arch = "aarch64")]
const VIRTIO0: PhysAddr = polyhal::pa!(0xa00_0000);

pub struct VirtIOBlock(UPSafeCell<VirtIOBlk<VirtioHal, MmioTransport>>);

lazy_static! {
    static ref QUEUE_FRAMES: UPSafeCell<Vec<FrameTracker>> = unsafe { UPSafeCell::new(Vec::new()) };
}

unsafe impl Sync for VirtIOBlock {}
unsafe impl Send for VirtIOBlock {}

impl BlockDevice for VirtIOBlock {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        self.0
            .exclusive_access()
            .read_blocks(block_id, buf)
            .expect("Error when reading VirtIOBlk");
    }
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        self.0
            .exclusive_access()
            .write_blocks(block_id, buf)
            .expect("Error when writing VirtIOBlk");
    }
}

impl VirtIOBlock {
    #[allow(unused)]
    pub fn new() -> Self {
        unsafe {
            Self(UPSafeCell::new(
                VirtIOBlk::<VirtioHal, MmioTransport>::new(
                    MmioTransport::new(NonNull::new_unchecked(VIRTIO0.get_mut_ptr()))
                        .expect("this is not a valid virtio device"),
                )
                .unwrap(),
            ))
        }
    }
}

pub struct VirtioHal;

unsafe impl Hal for VirtioHal {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (usize, NonNull<u8>) {
        // let mut ppn_base = PhysPage::new(0);
        // let mut paddr = PhysAddr::new(0);
        // for i in 0..pages {
        //     let frame = frame_alloc().unwrap();
        //     debug!("alloc paddr: {:?}", frame);
        //     if i == 0 {
        //         paddr = frame.paddr;
        //     };
        //     assert_eq!(frame.paddr.raw(), paddr.raw() + PAGE_SIZE);
        //     QUEUE_FRAMES.exclusive_access().push(frame);
        // }
        // unsafe { (paddr.raw(), NonNull::new_unchecked(paddr.get_mut_ptr())) }
        let frames = frames_alloc(pages).unwrap();
        let paddr = frames[0].paddr;
        QUEUE_FRAMES.exclusive_access().extend(frames);
        unsafe { (paddr.raw(), NonNull::new_unchecked(paddr.get_mut_ptr())) }
    }

    unsafe fn dma_dealloc(paddr: usize, _vaddr: NonNull<u8>, pages: usize) -> i32 {
        // trace!("dealloc DMA: paddr={:#x}, pages={}", paddr, pages);
        log::error!("dealloc paddr: {:?}", paddr);
        let mut pa = PhysAddr::new(paddr);
        for _ in 0..pages {
            frame_dealloc(pa);
            pa = pa + PAGE_SIZE;
        }
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: usize, _size: usize) -> NonNull<u8> {
        NonNull::new((usize::from(paddr) | VIRT_ADDR_START) as *mut u8).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> usize {
        buffer.as_ptr() as *mut u8 as usize - VIRT_ADDR_START
        // let pt = PageTable::current();
        // let paddr = pt.translate(VirtAddr::new(buffer.as_ptr() as *const u8 as usize)).expect("can't find vaddr").0;
        // paddr.addr()
    }

    unsafe fn unshare(_paddr: usize, _buffer: NonNull<[u8]>, _direction: BufferDirection) {
        // Nothing to do, as the host already has access to all memory and we didn't copy the buffer
        // anywhere else.
    }
}
