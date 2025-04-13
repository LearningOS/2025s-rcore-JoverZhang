//! Implementation of [`PageTableEntry`] and [`PageTable`].

use crate::mm::MapPermission;
use crate::task::current_user_token;

use super::{frame_alloc, FrameTracker, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;

bitflags! {
    /// page table entry flags
    pub struct PTEFlags: u8 {
        /// Valid
        const V = 1 << 0;
        /// Readable
        const R = 1 << 1;
        /// Writable
        const W = 1 << 2;
        /// eXecutable
        const X = 1 << 3;
        /// User
        const U = 1 << 4;
        /// Global
        const G = 1 << 5;
        /// Accessed
        const A = 1 << 6;
        /// Dirty
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
/// page table entry structure
pub struct PageTableEntry {
    /// bits of page table entry
    pub bits: usize,
}

impl PageTableEntry {
    /// Create a new page table entry
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }
    /// Create an empty page table entry
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    /// Get the physical page number from the page table entry
    pub fn ppn(&self) -> PhysPageNum {
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }
    /// Get the flags from the page table entry
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }
    /// The page pointered by page table entry is valid?
    pub fn is_valid(&self) -> bool {
        (self.flags() & PTEFlags::V) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is readable?
    pub fn readable(&self) -> bool {
        (self.flags() & PTEFlags::R) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is writable?
    pub fn writable(&self) -> bool {
        (self.flags() & PTEFlags::W) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is executable?
    pub fn executable(&self) -> bool {
        (self.flags() & PTEFlags::X) != PTEFlags::empty()
    }
}

/// page table structure
pub struct PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}

/// Assume that it won't oom when creating/mapping.
impl PageTable {
    /// Create a new page table
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        PageTable {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }
    /// Temporarily used to get arguments from user space.
    pub fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & ((1usize << 44) - 1)),
            frames: Vec::new(),
        }
    }
    /// Find PageTableEntry by VirtPageNum, create a frame for a 4KB page table if not exist
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                let frame = frame_alloc().unwrap();
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            }
            ppn = pte.ppn();
        }
        result
    }
    /// Find PageTableEntry by VirtPageNum
    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }
    /// set the map between virtual page number and physical page number
    #[allow(unused)]
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
    }
    /// remove the map between virtual page number and physical page number
    #[allow(unused)]
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        *pte = PageTableEntry::empty();
    }
    /// get the page table entry from the virtual page number
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| *pte)
    }
    /// get the token from the page table
    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}

/// Translate&Copy a ptr[u8] array with LENGTH len to a mutable u8 Vec through page table
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    do_translated_byte_buffer(token, ptr, len, MapPermission::empty()).unwrap()
}

fn do_translated_byte_buffer(
    token: usize,
    ptr: *const u8,
    len: usize,
    map_perm: MapPermission,
) -> Option<Vec<&'static mut [u8]>> {
    let page_table = PageTable::from_token(token);
    let start = ptr as usize;
    if !is_sv39_valid(start) {
        return None;
    }
    let end = start + len;
    if !is_sv39_valid(end) {
        return None;
    }
    let mut v = Vec::new();

    let mut start = VirtAddr::from(start);
    let end = VirtAddr::from(end);

    while start.0 < end.0 {
        let mut vpn = start.floor();

        let pte = if let Some(pte) = page_table.translate(vpn) {
            if !pte.is_valid() {
                return None;
            }
            if map_perm.contains(MapPermission::R) && !pte.readable() {
                return None;
            }
            if map_perm.contains(MapPermission::W) && !pte.writable() {
                return None;
            }
            if map_perm.contains(MapPermission::X) && !pte.executable() {
                return None;
            }
            pte
        } else {
            return None;
        };

        let ppn = pte.ppn();
        vpn.step();

        let mut end_va: VirtAddr = vpn.into();
        end_va = end_va.min(end);

        if end_va.page_offset() == 0 {
            v.push(&mut ppn.get_bytes_array()[start.page_offset()..]);
        } else {
            v.push(&mut ppn.get_bytes_array()[start.page_offset()..end_va.page_offset()]);
        }
        start = end_va.into();
    }
    Some(v)
}

/// Read data from the virtual address
pub fn read_va<T>(va: VirtAddr) -> Option<T> {
    let len = core::mem::size_of::<T>();

    let mut bufs = do_translated_byte_buffer(current_user_token(), va.0 as *const u8, len, MapPermission::R)?;

    let mut bytes = vec![0; len];
    let mut i = 0;

    for buf in bufs.iter_mut() {
        for p in buf.iter_mut() {
            bytes[i] = *p;

            i += 1;
        }
    }

    Some(unsafe { core::ptr::read_unaligned(bytes.as_ptr() as *const T) })
}

/// Write data to the virtual address
pub fn write_va<T>(va: VirtAddr, val: &T) -> Option<()> {
    // Take bytes from the value
    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(val as *const T as *const u8, core::mem::size_of::<T>())
    };

    // Get target physical address
    let mut bufs = do_translated_byte_buffer(current_user_token(), va.0 as *const u8, bytes.len(), MapPermission::W)?;
    let mut i = 0;

    // Write bytes to the target physical address
    for buf in bufs.iter_mut() {
        for p in buf.iter_mut() {
            *p = bytes[i];

            i += 1;
        }
    }

    Some(())
}

/// Check if the virtual address is valid in SV39
pub fn is_sv39_valid(addr: usize) -> bool {
    // Sign-extend bit 38: if bit 38 == 0 → upper bits must be 0
    //                     if bit 38 == 1 → upper bits must be 1
    (addr >> 39 == 0) || (addr >> 39 == (1 << 25) - 1) // 25 = 64 - 39
}
