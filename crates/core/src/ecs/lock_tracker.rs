//! Debug-only thread-local lock tracker for detecting same-thread deadlocks.
//!
//! `std::sync::RwLock` is not reentrant: acquiring a write lock while holding
//! a read lock on the same thread deadlocks silently. This module catches
//! that at the point of acquisition with a clear panic message.
//!
//! All functions are no-ops in release builds (`cfg(debug_assertions)`).

#[cfg(debug_assertions)]
use std::any::TypeId;
#[cfg(debug_assertions)]
use std::cell::RefCell;
#[cfg(debug_assertions)]
use std::collections::HashMap;

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy)]
struct LockState {
    read_count: u32,
    has_write: bool,
}

#[cfg(debug_assertions)]
thread_local! {
    static LOCKS: RefCell<HashMap<TypeId, LockState>> = RefCell::new(HashMap::new());
}

/// Record a read lock acquisition. Panics if a write lock is already held
/// on this type from the same thread (would deadlock).
#[cfg(debug_assertions)]
pub(crate) fn track_read(type_id: TypeId, type_name: &str) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        let entry = map.entry(type_id).or_insert(LockState {
            read_count: 0,
            has_write: false,
        });
        if entry.has_write {
            panic!(
                "ECS deadlock detected: attempted read lock on `{}` while a write lock \
                 is already held on the same thread. Drop the write query/resource first.",
                type_name,
            );
        }
        entry.read_count += 1;
    });
}

/// Record a write lock acquisition. Panics if any lock (read or write) is
/// already held on this type from the same thread (would deadlock).
#[cfg(debug_assertions)]
pub(crate) fn track_write(type_id: TypeId, type_name: &str) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        let entry = map.entry(type_id).or_insert(LockState {
            read_count: 0,
            has_write: false,
        });
        if entry.has_write {
            panic!(
                "ECS deadlock detected: attempted write lock on `{}` while a write lock \
                 is already held on the same thread. Drop the existing query/resource first.",
                type_name,
            );
        }
        if entry.read_count > 0 {
            panic!(
                "ECS deadlock detected: attempted write lock on `{}` while {} read lock(s) \
                 are held on the same thread. Drop all read queries/resources first.",
                type_name, entry.read_count,
            );
        }
        entry.has_write = true;
    });
}

/// Remove a read lock from tracking.
#[cfg(debug_assertions)]
pub(crate) fn untrack_read(type_id: TypeId) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        if let Some(entry) = map.get_mut(&type_id) {
            entry.read_count = entry.read_count.saturating_sub(1);
            if entry.read_count == 0 && !entry.has_write {
                map.remove(&type_id);
            }
        }
    });
}

/// Remove a write lock from tracking.
#[cfg(debug_assertions)]
pub(crate) fn untrack_write(type_id: TypeId) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        if let Some(entry) = map.get_mut(&type_id) {
            entry.has_write = false;
            if entry.read_count == 0 {
                map.remove(&type_id);
            }
        }
    });
}

// Release-build no-ops.

#[cfg(not(debug_assertions))]
#[inline(always)]
pub(crate) fn track_read(_type_id: std::any::TypeId, _type_name: &str) {}

#[cfg(not(debug_assertions))]
#[inline(always)]
pub(crate) fn track_write(_type_id: std::any::TypeId, _type_name: &str) {}

#[cfg(not(debug_assertions))]
#[inline(always)]
pub(crate) fn untrack_read(_type_id: std::any::TypeId) {}

#[cfg(not(debug_assertions))]
#[inline(always)]
pub(crate) fn untrack_write(_type_id: std::any::TypeId) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    struct FakeA;
    struct FakeB;

    #[test]
    fn multiple_reads_same_type_ok() {
        let id = TypeId::of::<FakeA>();
        track_read(id, "FakeA");
        track_read(id, "FakeA");
        track_read(id, "FakeA");
        untrack_read(id);
        untrack_read(id);
        untrack_read(id);
    }

    #[test]
    fn read_then_write_different_types_ok() {
        let id_a = TypeId::of::<FakeA>();
        let id_b = TypeId::of::<FakeB>();
        track_read(id_a, "FakeA");
        track_write(id_b, "FakeB");
        untrack_write(id_b);
        untrack_read(id_a);
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn read_then_write_same_type_panics() {
        let id = TypeId::of::<FakeA>();
        track_read(id, "FakeA");
        track_write(id, "FakeA"); // should panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn write_then_read_same_type_panics() {
        let id = TypeId::of::<FakeA>();
        track_write(id, "FakeA");
        track_read(id, "FakeA"); // should panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn write_then_write_same_type_panics() {
        let id = TypeId::of::<FakeA>();
        track_write(id, "FakeA");
        track_write(id, "FakeA"); // should panic
    }

    #[test]
    fn sequential_locks_after_drop_ok() {
        let id = TypeId::of::<FakeA>();
        track_write(id, "FakeA");
        untrack_write(id);
        // Should be fine now.
        track_read(id, "FakeA");
        untrack_read(id);
        track_write(id, "FakeA");
        untrack_write(id);
    }
}
