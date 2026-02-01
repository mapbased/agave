// Adapted from /home/chy/Git/art/src/ebr.rs
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct EbrState {
    current_epoch: AtomicUsize,
    active_counts: [AtomicUsize; 3],
    retired: [AtomicPtr<RetiredItem>; 3],
}

struct RetiredItem {
    ptr: *mut (),
    destructor: fn(*mut ()),
    next: AtomicPtr<RetiredItem>,
}

#[derive(Clone)]
pub struct AsyncEbr {
    state: Arc<EbrState>,
}

impl AsyncEbr {
    pub fn new() -> Self {
        AsyncEbr {
            state: Arc::new(EbrState {
                current_epoch: AtomicUsize::new(0),
                active_counts: [
                    AtomicUsize::new(0),
                    AtomicUsize::new(0),
                    AtomicUsize::new(0),
                ],
                retired: [
                    AtomicPtr::new(std::ptr::null_mut()),
                    AtomicPtr::new(std::ptr::null_mut()),
                    AtomicPtr::new(std::ptr::null_mut()),
                ],
            }),
        }
    }

    pub fn enter(&self) -> Guard {
        let epoch = self.state.current_epoch.load(Ordering::Acquire);
        self.state.active_counts[epoch].fetch_add(1, Ordering::SeqCst);

        Guard {
            epoch,
            ebr: self.clone(),
        }
    }

    fn try_advance_epoch(&self) {
        let curr = self.state.current_epoch.load(Ordering::Acquire);
        let next = (curr + 1) % 3;
        let prev = (curr + 2) % 3;

        // Ensure no readers are in the 'previous' epoch (which is the one we want to reclaim)
        if self.state.active_counts[prev].load(Ordering::Acquire) == 0 {
            if self
                .state
                .current_epoch
                .compare_exchange(curr, next, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                self.reclaim_epoch(prev);
            }
        }
    }

    fn reclaim_epoch(&self, epoch: usize) {
        let link = &self.state.retired[epoch];
        let mut ret = link.swap(std::ptr::null_mut(), Ordering::AcqRel);
        while !ret.is_null() {
            let item = unsafe { Box::from_raw(ret) };
            (item.destructor)(item.ptr);
            ret = item.next.load(Ordering::Acquire);
        }
    }
}

pub struct Guard {
    epoch: usize,
    ebr: AsyncEbr,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.ebr.state.active_counts[self.epoch].fetch_sub(1, Ordering::SeqCst);
        self.ebr.try_advance_epoch();
    }
}

impl Guard {
    pub fn retire<T: 'static>(&self, ptr: *mut T) {
        let destructor = |p| unsafe {
            drop(Box::from_raw(p as *mut T));
        };

        let item = RetiredItem {
            ptr: ptr as *mut (),
            destructor,
            next: AtomicPtr::new(std::ptr::null_mut()),
        };
        let link = &self.ebr.state.retired[self.epoch];
        let p = Box::into_raw(Box::new(item));
        let mut head = link.load(Ordering::Relaxed);
        loop {
            unsafe { (*p).next.store(head, Ordering::Relaxed) };

            match link.compare_exchange_weak(head, p, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => break,
                Err(h) => head = h,
            }
        }
    }
}
