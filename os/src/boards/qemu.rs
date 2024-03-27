pub const CLOCK_FREQ: usize = 12500000;
pub const MEMORY_END: usize = 0x8800_0000;

pub type BlockDeviceImpl = crate::drivers::block::VirtIOBlock;
