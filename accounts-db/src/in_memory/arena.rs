// Adapted from /home/chy/Git/art/src/arena.rs
#[cfg(unix)]
use libc::{
    mmap, MAP_ANONYMOUS, MAP_FIXED, MAP_NORESERVE, MAP_PRIVATE, PROT_NONE, PROT_READ, PROT_WRITE,
};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const PAGE_SIZE: usize = 4096;
const COMMIT_CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB granularity for physical commit

struct ArenaInner {
    next_index: u32,
    capacity: u32,
    committed_upto_bytes: usize,
    free_head: u32, // Linked list for recycled slots
    active_count: u32,
}

pub struct SubArena {
    pub base: *mut u8,
    slot_size: usize,
    reserved_size: usize,
    inner: Mutex<ArenaInner>,
}

unsafe impl Send for SubArena {}
unsafe impl Sync for SubArena {}

impl SubArena {
    pub fn new(slot_size: usize, reserved_size_gb: usize) -> Self {
        let reserved_size = reserved_size_gb * 1024 * 1024 * 1024;
        unsafe {
            #[cfg(unix)]
            let base = mmap(
                null_mut(),
                reserved_size,
                PROT_NONE,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
                -1,
                0,
            );

            if base == libc::MAP_FAILED {
                panic!(
                    "Virtual memory reservation failed for size {}GB",
                    reserved_size_gb
                );
            }

            Self {
                base: base as *mut u8,
                slot_size,
                reserved_size,
                inner: Mutex::new(ArenaInner {
                    next_index: 1, // 0 is reserved for null/none
                    capacity: (reserved_size / slot_size) as u32,
                    committed_upto_bytes: 0,
                    free_head: 0,
                    active_count: 0,
                }),
            }
        }
    }

    #[inline(always)]
    pub fn alloc(&self) -> u32 {
        let mut inner = self.inner.lock().unwrap();
        let idx = if inner.free_head != 0 {
            let idx = inner.free_head;
            unsafe {
                // Read next free index from the slot memory itself
                let node_ptr = self.base.add(idx as usize * self.slot_size) as *const u32;
                inner.free_head = *node_ptr;
            }
            idx
        } else {
            let idx = inner.next_index;
            inner.next_index += 1;

            let required_bytes = (idx as usize + 1) * self.slot_size;
            if required_bytes > inner.committed_upto_bytes {
                self.commit_more(&mut inner, required_bytes);
            }
            idx
        };

        // Initialize memory to zero
        unsafe {
            std::ptr::write_bytes(
                self.base.add(idx as usize * self.slot_size),
                0,
                self.slot_size,
            );
        }

        inner.active_count += 1;
        idx
    }

    #[inline(always)]
    pub fn free(&self, idx: u32) {
        if idx == 0 {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        unsafe {
            let node_ptr = self.base.add(idx as usize * self.slot_size) as *mut u32;
            *node_ptr = inner.free_head;
            inner.free_head = idx;
        }
        inner.active_count -= 1;
    }

    fn commit_more(&self, inner: &mut ArenaInner, required_bytes: usize) {
        let current_commit = inner.committed_upto_bytes;
        let new_commit = (required_bytes + COMMIT_CHUNK_SIZE - 1) & !(COMMIT_CHUNK_SIZE - 1);

        let start_addr = unsafe { self.base.add(current_commit) as usize };
        let size = new_commit - current_commit;

        unsafe {
            #[cfg(unix)]
            mmap(
                start_addr as *mut libc::c_void,
                size,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED,
                -1,
                0,
            );
        }
        inner.committed_upto_bytes = new_commit;
    }

    #[inline(always)]
    pub fn get_ptr(&self, idx: u32) -> *mut u8 {
        if idx == 0 {
            return null_mut();
        }
        unsafe { self.base.add(idx as usize * self.slot_size) }
    }
}
