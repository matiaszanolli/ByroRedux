//! Declared system access patterns (R7).
//!
//! Systems can opt into declaring which component storages and which
//! resources they read or write. The declaration is **runtime data**,
//! not a compile-time contract — it doesn't change what `World` lets
//! the system do, but it makes contention diagnosable before the
//! parallel scheduler is enabled (M27).
//!
//! ## Why this exists
//!
//! The per-storage `RwLock` + lock_tracker handle correctness already.
//! What they don't give you is a static answer to "which systems will
//! serialise on storage X?" After M27 turns on rayon, debugging "I
//! enabled the parallel feature and performance got weird" without
//! declared accesses means reading every system's body and inferring
//! its query shape. With declared accesses, [`Scheduler::access_report`]
//! can surface the conflict graph up-front.
//!
//! ## Three states per system
//!
//! - **Declared (empty)**: `Access::new()` — system claims it touches
//!   no ECS state. Runs in parallel with everything.
//! - **Declared (with claims)**: `Access::new().reads::<T>().writes::<U>()`
//!   — system claims a specific shape. Conflict analysis trusts it.
//! - **Undeclared**: `None`. Closures and not-yet-migrated systems.
//!   Conflict analysis treats every pairing involving this system as
//!   `Unknown` — the pessimistic fallback that is *not* "no conflict."
//!
//! Migration is incremental. The default for both `System::access`
//! and the scheduler's per-entry override is `None`, so existing
//! systems keep working unchanged; declaring is purely additive.

use std::any::TypeId;

use crate::ecs::resource::Resource;
use crate::ecs::storage::Component;

/// One declared access — either a component or a resource type.
///
/// `type_name` is captured at declaration time so the conflict report
/// can identify the type without the consumer having to round-trip the
/// `TypeId` back through a registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessEntry {
    pub type_id: TypeId,
    pub type_name: &'static str,
}

impl AccessEntry {
    fn for_type<T: 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
        }
    }
}

/// Declared access pattern — what a system promises to read and write.
///
/// Construct with [`Access::new`] and the builder methods. An empty
/// `Access` is a real claim ("I touch no ECS state"); see the module
/// docs for why this is distinct from the undeclared `None`.
#[derive(Debug, Default, Clone)]
pub struct Access {
    pub components_read: Vec<AccessEntry>,
    pub components_write: Vec<AccessEntry>,
    pub resources_read: Vec<AccessEntry>,
    pub resources_write: Vec<AccessEntry>,
}

impl Access {
    /// Start a new declaration with no claims. Use the builder methods
    /// below to add reads/writes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare that this system reads component storage `T`.
    pub fn reads<T: Component>(mut self) -> Self {
        self.components_read.push(AccessEntry::for_type::<T>());
        self
    }

    /// Declare that this system writes component storage `T`.
    pub fn writes<T: Component>(mut self) -> Self {
        self.components_write.push(AccessEntry::for_type::<T>());
        self
    }

    /// Declare that this system reads resource `T`.
    pub fn reads_resource<T: Resource>(mut self) -> Self {
        self.resources_read.push(AccessEntry::for_type::<T>());
        self
    }

    /// Declare that this system writes resource `T`.
    pub fn writes_resource<T: Resource>(mut self) -> Self {
        self.resources_write.push(AccessEntry::for_type::<T>());
        self
    }

    /// Total number of declared accesses across all four bags.
    pub fn len(&self) -> usize {
        self.components_read.len()
            + self.components_write.len()
            + self.resources_read.len()
            + self.resources_write.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Direction of a conflict between two declared accesses on the same
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// First system reads, second writes.
    ReadWrite,
    /// First system writes, second reads.
    WriteRead,
    /// Both systems write.
    WriteWrite,
}

/// One conflict between two systems.
#[derive(Debug, Clone, Copy)]
pub struct ConflictPair {
    pub type_name: &'static str,
    pub kind: ConflictKind,
    /// `true` if the conflicting type is a resource, `false` if it is
    /// a component storage.
    pub is_resource: bool,
}

/// Result of pairwise access analysis.
#[derive(Debug, Clone)]
pub enum AccessConflict {
    /// Both sides declared and no overlap was found.
    None,
    /// At least one side did not declare. Conservative — the runtime
    /// could touch anything, so the pair *could* serialise.
    Unknown {
        /// Which side(s) were undeclared, for diagnostics.
        left_undeclared: bool,
        right_undeclared: bool,
    },
    /// Both sides declared and at least one type collides under the
    /// reader/writer rules below.
    Conflict { pairs: Vec<ConflictPair> },
}

/// Compute the conflict between two systems' declared accesses.
///
/// Two declared accesses conflict if either:
/// - One side writes a type the other reads, OR
/// - Both sides write the same type.
///
/// If either side is `None`, returns [`AccessConflict::Unknown`] —
/// the analyzer cannot prove non-conflict against an undeclared system.
pub fn analyze_pair(left: Option<&Access>, right: Option<&Access>) -> AccessConflict {
    let (left, right) = match (left, right) {
        (Some(l), Some(r)) => (l, r),
        (l, r) => {
            return AccessConflict::Unknown {
                left_undeclared: l.is_none(),
                right_undeclared: r.is_none(),
            };
        }
    };

    let mut pairs = Vec::new();

    // Components.
    collect_overlap(
        &left.components_write,
        &right.components_read,
        ConflictKind::WriteRead,
        false,
        &mut pairs,
    );
    collect_overlap(
        &left.components_read,
        &right.components_write,
        ConflictKind::ReadWrite,
        false,
        &mut pairs,
    );
    collect_overlap(
        &left.components_write,
        &right.components_write,
        ConflictKind::WriteWrite,
        false,
        &mut pairs,
    );

    // Resources.
    collect_overlap(
        &left.resources_write,
        &right.resources_read,
        ConflictKind::WriteRead,
        true,
        &mut pairs,
    );
    collect_overlap(
        &left.resources_read,
        &right.resources_write,
        ConflictKind::ReadWrite,
        true,
        &mut pairs,
    );
    collect_overlap(
        &left.resources_write,
        &right.resources_write,
        ConflictKind::WriteWrite,
        true,
        &mut pairs,
    );

    if pairs.is_empty() {
        AccessConflict::None
    } else {
        AccessConflict::Conflict { pairs }
    }
}

fn collect_overlap(
    a: &[AccessEntry],
    b: &[AccessEntry],
    kind: ConflictKind,
    is_resource: bool,
    out: &mut Vec<ConflictPair>,
) {
    for left_entry in a {
        for right_entry in b {
            if left_entry.type_id == right_entry.type_id {
                out.push(ConflictPair {
                    type_name: left_entry.type_name,
                    kind,
                    is_resource,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::resource::Resource;
    use crate::ecs::sparse_set::SparseSetStorage;
    use crate::ecs::storage::Component;

    struct Health(#[allow(dead_code)] f32);
    impl Component for Health {
        type Storage = SparseSetStorage<Self>;
    }

    struct Position {
        _x: f32,
    }
    impl Component for Position {
        type Storage = SparseSetStorage<Self>;
    }

    struct Clock(#[allow(dead_code)] f32);
    impl Resource for Clock {}

    #[test]
    fn empty_accesses_report_no_conflict() {
        let l = Access::new();
        let r = Access::new();
        assert!(matches!(analyze_pair(Some(&l), Some(&r)), AccessConflict::None));
    }

    #[test]
    fn write_vs_read_same_component_is_conflict() {
        let l = Access::new().writes::<Health>();
        let r = Access::new().reads::<Health>();
        match analyze_pair(Some(&l), Some(&r)) {
            AccessConflict::Conflict { pairs } => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0].kind, ConflictKind::WriteRead);
                assert!(!pairs[0].is_resource);
                assert!(pairs[0].type_name.ends_with("Health"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn read_vs_read_same_component_is_no_conflict() {
        let l = Access::new().reads::<Health>();
        let r = Access::new().reads::<Health>();
        assert!(matches!(analyze_pair(Some(&l), Some(&r)), AccessConflict::None));
    }

    #[test]
    fn write_vs_write_same_component_is_conflict() {
        let l = Access::new().writes::<Health>();
        let r = Access::new().writes::<Health>();
        match analyze_pair(Some(&l), Some(&r)) {
            AccessConflict::Conflict { pairs } => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0].kind, ConflictKind::WriteWrite);
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn unrelated_components_no_conflict() {
        let l = Access::new().writes::<Health>();
        let r = Access::new().writes::<Position>();
        assert!(matches!(analyze_pair(Some(&l), Some(&r)), AccessConflict::None));
    }

    #[test]
    fn resource_writeread_is_conflict() {
        let l = Access::new().writes_resource::<Clock>();
        let r = Access::new().reads_resource::<Clock>();
        match analyze_pair(Some(&l), Some(&r)) {
            AccessConflict::Conflict { pairs } => {
                assert_eq!(pairs.len(), 1);
                assert!(pairs[0].is_resource);
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn undeclared_left_is_unknown() {
        let r = Access::new().writes::<Health>();
        match analyze_pair(None, Some(&r)) {
            AccessConflict::Unknown {
                left_undeclared,
                right_undeclared,
            } => {
                assert!(left_undeclared);
                assert!(!right_undeclared);
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn undeclared_both_is_unknown_with_both_flags() {
        match analyze_pair(None, None) {
            AccessConflict::Unknown {
                left_undeclared,
                right_undeclared,
            } => {
                assert!(left_undeclared);
                assert!(right_undeclared);
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn multiple_overlaps_listed_individually() {
        let l = Access::new()
            .writes::<Health>()
            .writes::<Position>()
            .writes_resource::<Clock>();
        let r = Access::new()
            .reads::<Health>()
            .reads::<Position>()
            .reads_resource::<Clock>();
        match analyze_pair(Some(&l), Some(&r)) {
            AccessConflict::Conflict { pairs } => {
                assert_eq!(pairs.len(), 3);
                let resource_count = pairs.iter().filter(|p| p.is_resource).count();
                assert_eq!(resource_count, 1);
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn len_and_is_empty() {
        assert!(Access::new().is_empty());
        assert_eq!(
            Access::new()
                .reads::<Health>()
                .writes::<Position>()
                .reads_resource::<Clock>()
                .len(),
            3
        );
    }
}
