//! Shared safety helpers for ECS hierarchy traversals.

/// Bounds one hierarchy walk using the current entity population and child
/// references.
///
/// A valid parent/children tree visits at most one entry per live entity. The
/// child-reference term leaves room for duplicated edges while still making a
/// malformed cyclic graph finite. Callers additionally use a visited set so a
/// cycle is rejected before it can multiply the pending work.
#[derive(Debug)]
pub struct HierarchyTraversalGuard {
    remaining: usize,
}

impl HierarchyTraversalGuard {
    /// Creates a guard for a walk over the current hierarchy snapshot.
    pub fn new(entity_count: usize, child_reference_count: usize) -> Self {
        Self {
            remaining: entity_count
                .saturating_add(child_reference_count)
                .saturating_add(1),
        }
    }

    /// Consumes one traversal step, returning `false` after the finite budget
    /// is exhausted.
    pub fn step(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}
