use super::vpn_range::VPNRange;
use super::{frame_alloc, FrameTracker};
use crate::config::{PAGE_SIZE, USER_STACK_SIZE};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use polyhal::pagetable::{MappingFlags, MappingSize, PageTable, PageTableWrapper};
use polyhal::addr::{PhysPage, VirtAddr, VirtPage};

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
        let mut memory_set = Self::new_bare();
        // map trampoline
        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPage::new(0);
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
                max_end_vpn = map_area.vpn_range.get_end();
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        // map user stack with U flags
        let max_end_va: VirtAddr = max_end_vpn.into();
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
        let mut memory_set = Self::new_bare();
        // let pt = Arc::new(PageTableWrapper::alloc());
        // info!("pt");
        // let mut memory_set = MemorySet {
        //     page_table: pt,
        //     areas: Vec::new(),
        // };
        // copy data sections/trap_context/user_stack
        for area in user_space.areas.iter() {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);
            // copy data from another space
            for vpn in area.vpn_range {
                let src_ppn = user_space.translate(vpn).unwrap().0;
                let dst_ppn = memory_set.translate(vpn).unwrap().0;
                // dst_ppn
                //     .get_bytes_array()
                //     .copy_from_slice(src_ppn.get_bytes_array());
                dst_ppn.get_buffer().copy_from_slice(src_ppn.get_buffer())
            }
        }
        memory_set
    }
    pub fn activate(&self) {
        self.page_table.change();
    }
    pub fn translate(&self, vpn: VirtPage) -> Option<(PhysPage, MappingFlags)> {
        self.page_table
            .translate(vpn.into())
            .map(|(pa, flags)| (pa.into(), flags))
    }
    pub fn recycle_data_pages(&mut self) {
        //*self = Self::new_bare();
        self.areas.clear();
    }
}

pub struct MapArea {
    pub vpn_range: VPNRange,
    data_frames: BTreeMap<PhysPage, FrameTracker>,
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
        let start_vpn: VirtPage = start_va.floor().into();
        let end_vpn: VirtPage = end_va.ceil().into();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }
    pub fn from_another(another: &MapArea) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
        }
    }
    pub fn map(&mut self, page_table: &Arc<PageTableWrapper>) {
        for vpn in self.vpn_range {
            // self.map_one(page_table, vpn);
            let p_tracker = frame_alloc().expect("can't allocate frame");
            page_table.map_page(vpn, p_tracker.ppn, self.map_perm.into(), MappingSize::Page4KB);
            self.data_frames.insert(p_tracker.ppn, p_tracker);
        }
    }
    /// data: start-aligned but maybe with shorter length
    /// assume that all frames were cleared before
    pub fn copy_data(&mut self, page_table: &Arc<PageTableWrapper>, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut PhysPage::from(page_table.translate(current_vpn.into()).unwrap().0)
                .get_buffer()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            // current_vpn.step();
            current_vpn = current_vpn + 1;
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType {
    Identical,
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
