use super::vpn_range::VAddrRange;
use super::{frame_alloc, FrameTracker};
use crate::config::{PAGE_SIZE, USER_STACK_SIZE};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::*;
use polyhal::pagetable::{MappingFlags, MappingSize, PageTable, PageTableWrapper};
use polyhal::{PhysAddr, VirtAddr};
pub struct MemorySet {
    page_table: Arc<PageTableWrapper>,
    areas: Vec<MapArea>,
}

impl MemorySet {
    pub fn new_bare() -> Self {
        Self {
            page_table: Arc::new(PageTableWrapper::alloc()),
            areas: Vec::new(),
        }
    }
    pub fn token(&self) -> PageTable {
        self.page_table.0
    }
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&self.page_table, data);
        }
        self.areas.push(map_area);
    }
    /// Include sections in elf and trampoline and TrapContext and user stack,
    /// also returns user_sp and entry point.
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        trace!("os::mm::MemorySet::from_elf");
        let mut memory_set = Self::new_bare();
        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_va = VirtAddr::new(0);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_va = map_area.vaddr_range.get_end();
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        // map user stack with U flags
        let mut user_stack_bottom: usize = max_end_va.into();
        // guard page
        user_stack_bottom += PAGE_SIZE;
        let user_stack_top = user_stack_bottom + USER_STACK_SIZE;
        memory_set.push(
            MapArea::new(
                user_stack_bottom.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        // map TrapContext
        (
            memory_set,
            user_stack_top,
            elf.header.pt2.entry_point() as usize,
        )
    }
    pub fn from_existed_user(user_space: &MemorySet) -> MemorySet {
        trace!("os::mm::MemorySet::from_existed_user");
        let mut memory_set = Self::new_bare();
        // copy data sections/trap_context/user_stack
        for area in user_space.areas.iter() {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);
            // copy data from another space
            for vpn in area.vaddr_range {
                let src = user_space.translate(vpn).unwrap().0;
                let dst = memory_set.translate(vpn).unwrap().0;
                // dst_ppn.get_buffer().copy_from_slice(src_ppn.get_buffer())
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        src.get_ptr::<u8>(),
                        dst.get_mut_ptr(),
                        PAGE_SIZE,
                    );
                }
            }
        }
        memory_set
    }
    pub fn activate(&self) {
        self.page_table.change();
    }
    pub fn translate(&self, vaddr: VirtAddr) -> Option<(PhysAddr, MappingFlags)> {
        self.page_table
            .translate(vaddr)
            .map(|(pa, flags)| (pa.into(), flags))
    }
    pub fn recycle_data_pages(&mut self) {
        //*self = Self::new_bare();
        self.areas.clear();
    }
}

pub struct MapArea {
    pub vaddr_range: VAddrRange,
    data_frames: BTreeMap<VirtAddr, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtAddr = start_va.floor();
        let end_vpn: VirtAddr = end_va.ceil();
        Self {
            vaddr_range: VAddrRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }
    pub fn from_another(another: &MapArea) -> Self {
        Self {
            vaddr_range: VAddrRange::new(
                another.vaddr_range.get_start(),
                another.vaddr_range.get_end(),
            ),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
        }
    }
    pub fn map(&mut self, page_table: &Arc<PageTableWrapper>) {
        trace!("os::mm::memory_set::MapArea::map");
        for vaddr in self.vaddr_range {
            // self.map_one(page_table, vpn);
            let p_tracker = frame_alloc().expect("can't allocate frame");
            page_table.map_page(
                vaddr,
                p_tracker.paddr,
                self.map_perm.into(),
                MappingSize::Page4KB,
            );
            self.data_frames.insert(vaddr, p_tracker);
        }
    }

    /// Unmap page area
    #[allow(unused)]
    pub fn unmap(&mut self, page_table: &Arc<PageTableWrapper>) {
        trace!("os::mm::memory_set::MapArea::unmap");
        for vpn in self.vaddr_range {
            page_table.unmap_page(vpn);
        }
    }

    /// data: start-aligned but maybe with shorter length
    /// assume that all frames were cleared before
    pub fn copy_data(&mut self, page_table: &Arc<PageTableWrapper>, data: &[u8]) {
        trace!("os::mm::memory_set::MapArea::copy_data");
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut curr_vaddr = self.vaddr_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(curr_vaddr.into())
                .unwrap()
                .0
                .slice_mut_with_len(src.len());
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            // current_vpn.step();
            curr_vaddr = curr_vaddr + PAGE_SIZE;
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType {
    //  Identical, not used now
    Framed,
}

bitflags! {
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}

impl Into<MappingFlags> for MapPermission {
    fn into(self) -> MappingFlags {
        let mut flags = MappingFlags::empty();
        if self.contains(MapPermission::R) {
            flags |= MappingFlags::R;
        }
        if self.contains(MapPermission::W) {
            flags |= MappingFlags::W;
        }
        if self.contains(MapPermission::X) {
            flags |= MappingFlags::X;
        }
        if self.contains(MapPermission::U) {
            flags |= MappingFlags::U;
        }
        flags
    }
}
