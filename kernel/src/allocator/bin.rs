use core::alloc::{AllocErr, Layout, Opaque};
use core::ptr::NonNull;

use allocator::util::*;
use allocator::linked_list::LinkedList;

const BIN_COUNT: usize = 32;

/// A simple allocator that allocates based on size classes.
#[derive(Debug)]
pub struct Allocator {
    bin: [LinkedList; BIN_COUNT],

    current: usize,
    end: usize,
}

impl Allocator {
    /// Creates a new bin allocator that will allocate memory from the region
    /// starting at address `start` and ending at address `end`.
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            bin: [LinkedList::new(); BIN_COUNT],
            current: start,
            end,
        }
    }

    /// Allocates memory. Returns a pointer meeting the size and alignment
    /// properties of `layout.size()` and `layout.align()`.
    ///
    /// If this method returns an `Ok(addr)`, `addr` will be non-null address
    /// pointing to a block of storage suitable for holding an instance of
    /// `layout`. In particular, the block will be at least `layout.size()`
    /// bytes large and will be aligned to `layout.align()`. The returned block
    /// of storage may or may not have its contents initialized or zeroed.
    ///
    /// # Safety
    ///
    /// The _caller_ must ensure that `layout.size() > 0` and that
    /// `layout.align()` is a power of two. Parameters not meeting these
    /// conditions may result in undefined behavior.
    ///
    /// # Errors
    ///
    /// Returning `Err` indicates that either memory is exhausted
    /// (`AllocError::Exhausted`) or `layout` does not meet this allocator's
    /// size or alignment constraints (`AllocError::Unsupported`).
    pub fn alloc(&mut self, layout: Layout) -> Result<NonNull<Opaque>, AllocErr> {
        let bin = (layout.size() + layout.align())
            .next_power_of_two()
            .trailing_zeros() as usize;
        if bin < 3 || bin >= BIN_COUNT + 3 {
            return Err(AllocErr);
        }

        let bin = &mut self.bin[bin - 3];

        if let Some(addr) = bin.pop() {
            let addr = align_up(addr as usize, layout.align());
            unsafe { Ok(NonNull::new_unchecked(addr as *mut u8).as_opaque()) }
        } else {
            let start = align_up(self.current, layout.align());
            let end = start + layout.size();
            if end >= self.end {
                Err(AllocErr)
            } else {
                self.current = end;
                unsafe { Ok(NonNull::new_unchecked(start as *mut u8).as_opaque()) }
            }
        }
    }

    /// Deallocates the memory referenced by `ptr`.
    ///
    /// # Safety
    ///
    /// The _caller_ must ensure the following:
    ///
    ///   * `ptr` must denote a block of memory currently allocated via this
    ///     allocator
    ///   * `layout` must properly represent the original layout used in the
    ///     allocation call that returned `ptr`
    ///
    /// Parameters not meeting these conditions may result in undefined
    /// behavior.
    pub fn dealloc(&mut self, ptr: NonNull<Opaque>, layout: Layout) {
        let bin = (layout.size() + layout.align())
            .next_power_of_two()
            .trailing_zeros() as usize;
        let bin = &mut self.bin[bin - 3];
        unsafe { bin.push(ptr.as_ptr() as *mut usize); }
    }
}
