use polyhal::{pagetable::PAGE_SIZE, VirtAddr};

#[derive(Copy, Clone, Debug)]
pub struct VAddrRange {
    l: VirtAddr,
    r: VirtAddr,
}
impl VAddrRange {
    pub fn new(start: VirtAddr, end: VirtAddr) -> Self {
        assert!(start <= end, "start {:?} > end {:?}!", start, end);
        Self { l: start, r: end }
    }
    pub fn get_start(&self) -> VirtAddr {
        self.l
    }
    pub fn get_end(&self) -> VirtAddr {
        self.r
    }
}
impl IntoIterator for VAddrRange {
    type Item = VirtAddr;
    type IntoIter = SimpleRangeIterator;
    fn into_iter(self) -> Self::IntoIter {
        SimpleRangeIterator::new(self.l, self.r)
    }
}
pub struct SimpleRangeIterator {
    current: VirtAddr,
    end: VirtAddr,
}
impl SimpleRangeIterator {
    pub fn new(l: VirtAddr, r: VirtAddr) -> Self {
        Self { current: l, end: r }
    }
}
impl Iterator for SimpleRangeIterator {
    type Item = VirtAddr;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.end {
            None
        } else {
            let t = self.current;
            self.current = self.current + PAGE_SIZE;
            // let t = self.current;
            // self.current.step();
            Some(t)
        }
    }
}
