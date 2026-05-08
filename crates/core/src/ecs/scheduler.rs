//! Stage-based parallel scheduler.
//!
//! Systems are assigned to **stages** that run sequentially in a fixed order.
//! Within each stage, systems run **in parallel** via rayon (when the
//! `parallel-scheduler` feature is enabled). The `World`'s per-storage
//! `RwLock` design naturally serialises conflicting accesses — no explicit
//! dependency declarations are needed.
//!
//! Systems added with `add_exclusive` run alone *after* the parallel batch
//! in their stage completes. Use this for cleanup or barrier systems.

use super::access::{analyze_pair, Access, AccessConflict};
use super::system::System;
use super::world::World;
use std::collections::BTreeMap;

#[cfg(feature = "parallel-scheduler")]
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};

/// Execution stage — stages run sequentially in discriminant order.
///
/// Within a stage, all non-exclusive systems run in parallel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Input handling, camera, timers — runs first.
    Early = 0,
    /// Core gameplay: animation, AI, scripting.
    Update = 1,
    /// Transform propagation — sees results of Update.
    PostUpdate = 2,
    /// Physics sync — sees propagated transforms.
    Physics = 3,
    /// Stats, cleanup — runs last.
    Late = 4,
}

/// One scheduled system plus its declared access (R7).
///
/// `declared_override` lets the scheduler attach an [`Access`]
/// declaration at registration time — useful for closures, which
/// can't override `System::access` themselves. When `None`, the
/// scheduler falls back to `system.access()` (still optional).
struct SystemEntry {
    system: Box<dyn System>,
    declared_override: Option<Access>,
}

impl SystemEntry {
    /// Resolve the effective declaration, preferring the registration-
    /// site override. `None` means "undeclared" and propagates as
    /// `AccessConflict::Unknown` through analysis.
    fn declared_access(&self) -> Option<Access> {
        if let Some(over) = &self.declared_override {
            return Some(over.clone());
        }
        self.system.access()
    }
}

/// Per-stage system storage.
struct StageData {
    /// Systems that run in parallel within this stage.
    parallel: Vec<SystemEntry>,
    /// Systems that run sequentially *after* the parallel batch completes.
    exclusive: Vec<SystemEntry>,
}

impl StageData {
    fn new() -> Self {
        Self {
            parallel: Vec::new(),
            exclusive: Vec::new(),
        }
    }

    fn all_names(&self) -> impl Iterator<Item = &str> {
        self.parallel
            .iter()
            .chain(self.exclusive.iter())
            .map(|e| e.system.name())
    }
}

pub struct Scheduler {
    stages: BTreeMap<Stage, StageData>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            stages: BTreeMap::new(),
        }
    }

    /// Add a system to [`Stage::Update`] (backward-compatible default).
    ///
    /// Systems added via `add()` run in parallel with other systems in
    /// the same stage. For explicit stage assignment, use [`add_to`].
    pub fn add<S: System + 'static>(&mut self, system: S) -> &mut Self {
        self.add_to(Stage::Update, system)
    }

    /// Add a system to a specific stage.
    ///
    /// Within the stage, this system runs in parallel with other
    /// non-exclusive systems. Use [`add_exclusive`] for systems that
    /// must run alone after the parallel batch.
    pub fn add_to<S: System + 'static>(&mut self, stage: Stage, system: S) -> &mut Self {
        let name = system.name().to_string();
        if self.has_system(&name) {
            // Duplicate name push is intentional for use-cases like
            // registering multiple closures of the same signature —
            // `std::any::type_name` collapses all matching closures to
            // a single name so the scheduler can't distinguish them.
            // For named struct systems (`impl System with fn name`) the
            // warning catches honest mistakes. See #312 +
            // `try_add_to` for the strict form.
            log::warn!(
                "Scheduler: duplicate system name '{}' in stage {:?} \
                 (closures with identical signatures share type_name; \
                 for named structs, prefer `try_add_to`)",
                name,
                stage,
            );
        }
        self.stages
            .entry(stage)
            .or_insert_with(StageData::new)
            .parallel
            .push(SystemEntry {
                system: Box::new(system),
                declared_override: None,
            });
        self
    }

    /// Add a parallel system to `stage` with an explicit access
    /// declaration (R7).
    ///
    /// Overrides whatever `System::access` returns. Intended for
    /// closure systems, which can't override the trait method
    /// themselves; struct systems usually declare via `impl System`
    /// directly. The override is otherwise identical to [`add_to`] —
    /// no duplicate-name rejection (use [`try_add_to_with_access`] for
    /// that).
    pub fn add_to_with_access<S: System + 'static>(
        &mut self,
        stage: Stage,
        system: S,
        access: Access,
    ) -> &mut Self {
        let name = system.name().to_string();
        if self.has_system(&name) {
            log::warn!(
                "Scheduler: duplicate system name '{}' in stage {:?} \
                 (with declared access — see add_to)",
                name,
                stage,
            );
        }
        self.stages
            .entry(stage)
            .or_insert_with(StageData::new)
            .parallel
            .push(SystemEntry {
                system: Box::new(system),
                declared_override: Some(access),
            });
        self
    }

    /// Add a parallel system to `stage`, rejecting duplicates by name.
    ///
    /// Returns `Err(name)` when a system with the same name is already
    /// registered; the system is NOT added. Prefer this in engine
    /// setup when you want a loud failure if someone accidentally
    /// registers the same named struct twice (#312). Closures share a
    /// `type_name` with siblings of the same signature, so use
    /// `add_to` for those.
    pub fn try_add_to<S: System + 'static>(
        &mut self,
        stage: Stage,
        system: S,
    ) -> Result<&mut Self, String> {
        let name = system.name().to_string();
        if self.has_system(&name) {
            return Err(name);
        }
        self.stages
            .entry(stage)
            .or_insert_with(StageData::new)
            .parallel
            .push(SystemEntry {
                system: Box::new(system),
                declared_override: None,
            });
        Ok(self)
    }

    /// Strict sibling of [`add_to_with_access`] — rejects duplicate names.
    pub fn try_add_to_with_access<S: System + 'static>(
        &mut self,
        stage: Stage,
        system: S,
        access: Access,
    ) -> Result<&mut Self, String> {
        let name = system.name().to_string();
        if self.has_system(&name) {
            return Err(name);
        }
        self.stages
            .entry(stage)
            .or_insert_with(StageData::new)
            .parallel
            .push(SystemEntry {
                system: Box::new(system),
                declared_override: Some(access),
            });
        Ok(self)
    }

    /// Add an exclusive system to a specific stage.
    ///
    /// Exclusive systems run sequentially *after* all parallel systems
    /// in the same stage have completed. Use this for barrier or cleanup
    /// systems that must see the results of the parallel batch.
    pub fn add_exclusive<S: System + 'static>(&mut self, stage: Stage, system: S) -> &mut Self {
        let name = system.name().to_string();
        if self.has_system(&name) {
            // Same-name-allowed contract as add_to — see its comment.
            log::warn!(
                "Scheduler: duplicate exclusive system name '{}' in stage {:?} \
                 (prefer `try_add_exclusive` for named struct systems)",
                name,
                stage,
            );
        }
        self.stages
            .entry(stage)
            .or_insert_with(StageData::new)
            .exclusive
            .push(SystemEntry {
                system: Box::new(system),
                declared_override: None,
            });
        self
    }

    /// Add an exclusive system to `stage`, rejecting duplicates by name.
    ///
    /// Sibling of `try_add_to` for the exclusive phase. See #312.
    pub fn try_add_exclusive<S: System + 'static>(
        &mut self,
        stage: Stage,
        system: S,
    ) -> Result<&mut Self, String> {
        let name = system.name().to_string();
        if self.has_system(&name) {
            return Err(name);
        }
        self.stages
            .entry(stage)
            .or_insert_with(StageData::new)
            .exclusive
            .push(SystemEntry {
                system: Box::new(system),
                declared_override: None,
            });
        Ok(self)
    }

    /// Run all systems: stages in order, parallel within each stage.
    ///
    /// # Re-entry
    ///
    /// `run` takes `&mut self`. Systems receive `&World` (interior-
    /// mutable via `RwLock`) and have no safe path to reach the
    /// `Scheduler` instance — `Scheduler` is owned by the `App`
    /// struct and is intentionally **not** stored as a `Resource`.
    /// Re-entry is therefore structurally impossible from inside
    /// any system body. Future maintainers must keep `Scheduler`
    /// out of the resource map (and out of `Arc<Mutex<_>>` wrappers
    /// behind hot-reload schemes); promoting it would compile, but
    /// `system → scheduler.run` would then panic at `BorrowMutError`
    /// because the outer `&mut self` is held across the inner call.
    /// See #868.
    ///
    /// # Panic propagation
    ///
    /// A panic in any system aborts the *current frame*; the
    /// scheduler does not isolate or recover. Specifics differ by
    /// build:
    ///
    /// - **`parallel-scheduler` enabled (default)**: rayon's
    ///   `par_iter_mut().for_each` does not cancel sibling tasks on
    ///   panic. Every parallel system in the *current* stage runs
    ///   to completion (or panics independently); rayon then
    ///   propagates the *first* observed panic to this thread when
    ///   the for_each returns. Subsequent stages and the current
    ///   stage's exclusive phase do not run for the rest of the
    ///   frame. See rayon-core's panic policy.
    /// - **`parallel-scheduler` disabled**: the plain `for` loop
    ///   short-circuits on the first panic. Remaining parallel-phase
    ///   systems and *all* exclusive-phase systems in the same
    ///   stage do not run for that frame, identical to subsequent
    ///   stages.
    ///
    /// In either build, a panic propagates out of `run` to the
    /// caller's frame loop. The engine's main loop today doesn't
    /// catch it — a panicking system terminates the process. If a
    /// future build adds per-frame `catch_unwind` recovery, the
    /// recovered frame will see partial state from the failed stage
    /// (some parallel systems ran, some didn't; mid-stage exclusive
    /// systems are guaranteed to have not run). See #867.
    pub fn run(&mut self, world: &World, dt: f32) {
        for (_stage, data) in &mut self.stages {
            // Phase 1: run parallel systems concurrently.
            #[cfg(feature = "parallel-scheduler")]
            {
                data.parallel
                    .par_iter_mut()
                    .for_each(|entry| entry.system.run(world, dt));
            }
            #[cfg(not(feature = "parallel-scheduler"))]
            {
                for entry in &mut data.parallel {
                    entry.system.run(world, dt);
                }
            }
            // Phase 2: run exclusive systems sequentially.
            for entry in &mut data.exclusive {
                entry.system.run(world, dt);
            }
        }
    }

    /// Returns the names of all registered systems, in stage order.
    ///
    /// Within each stage, parallel systems appear first, then exclusive.
    pub fn system_names(&self) -> Vec<&str> {
        self.stages.values().flat_map(|d| d.all_names()).collect()
    }

    fn has_system(&self, name: &str) -> bool {
        self.stages
            .values()
            .any(|d| d.all_names().any(|n| n == name))
    }

    /// Build a per-stage report of declared accesses + parallel-pair
    /// conflicts (R7).
    ///
    /// Computed on demand — no caching, since the scheduler's lifetime
    /// is the whole process and the report is only requested by debug
    /// commands and one-shot diagnostics. Walks every parallel-stage
    /// pair, classifying it as `None`, `Conflict { pairs }`, or
    /// `Unknown` (when one or both sides are undeclared).
    ///
    /// Exclusive-phase systems are listed but not paired against each
    /// other (they run serially after the parallel batch, so they can
    /// never collide).
    pub fn access_report(&self) -> AccessReport {
        let mut stages = Vec::new();
        for (stage, data) in &self.stages {
            let mut systems = Vec::with_capacity(data.parallel.len() + data.exclusive.len());
            for entry in &data.parallel {
                systems.push(SystemAccessRow {
                    name: entry.system.name().to_string(),
                    declared: entry.declared_access(),
                    is_exclusive: false,
                });
            }
            for entry in &data.exclusive {
                systems.push(SystemAccessRow {
                    name: entry.system.name().to_string(),
                    declared: entry.declared_access(),
                    is_exclusive: true,
                });
            }

            let mut conflicts = Vec::new();
            for i in 0..data.parallel.len() {
                for j in (i + 1)..data.parallel.len() {
                    let left = data.parallel[i].declared_access();
                    let right = data.parallel[j].declared_access();
                    let conflict = analyze_pair(left.as_ref(), right.as_ref());
                    if !matches!(conflict, AccessConflict::None) {
                        conflicts.push(StageConflictRow {
                            left: data.parallel[i].system.name().to_string(),
                            right: data.parallel[j].system.name().to_string(),
                            conflict,
                        });
                    }
                }
            }

            stages.push(StageReport {
                stage: *stage,
                systems,
                conflicts,
            });
        }
        AccessReport { stages }
    }
}

/// One system's row in an [`AccessReport`].
#[derive(Debug, Clone)]
pub struct SystemAccessRow {
    pub name: String,
    pub declared: Option<Access>,
    pub is_exclusive: bool,
}

/// One conflicting pair within a stage.
#[derive(Debug, Clone)]
pub struct StageConflictRow {
    pub left: String,
    pub right: String,
    pub conflict: AccessConflict,
}

/// One stage's slice of an [`AccessReport`].
#[derive(Debug, Clone)]
pub struct StageReport {
    pub stage: Stage,
    /// Every system in the stage, parallel first then exclusive,
    /// in registration order. Each row's `declared` is the merged
    /// declaration (registration-site override or `System::access()`).
    pub systems: Vec<SystemAccessRow>,
    /// Pairwise classification for the parallel batch. Pairs with
    /// `AccessConflict::None` are omitted; only the "interesting"
    /// rows (Conflict + Unknown) appear.
    pub conflicts: Vec<StageConflictRow>,
}

/// Full scheduler-wide access summary returned by
/// [`Scheduler::access_report`].
#[derive(Debug, Clone, Default)]
pub struct AccessReport {
    pub stages: Vec<StageReport>,
}

impl AccessReport {
    /// Total number of systems across all stages.
    pub fn system_count(&self) -> usize {
        self.stages.iter().map(|s| s.systems.len()).sum()
    }

    /// How many systems still have `declared = None`. Drives the "X
    /// systems undeclared" line in the console command.
    pub fn undeclared_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|s| s.systems.iter())
            .filter(|row| row.declared.is_none())
            .count()
    }

    /// Total Conflict rows across all stages (excludes Unknown).
    pub fn known_conflict_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|s| s.conflicts.iter())
            .filter(|c| matches!(c.conflict, AccessConflict::Conflict { .. }))
            .count()
    }

    /// Total Unknown rows across all stages.
    pub fn unknown_pair_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|s| s.conflicts.iter())
            .filter(|c| matches!(c.conflict, AccessConflict::Unknown { .. }))
            .count()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::packed::PackedStorage;
    use crate::ecs::sparse_set::SparseSetStorage;
    use crate::ecs::storage::Component;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct Health(f32);
    impl Component for Health {
        type Storage = SparseSetStorage<Self>;
    }

    struct Position {
        x: f32,
        y: f32,
    }
    impl Component for Position {
        type Storage = PackedStorage<Self>;
    }

    struct Velocity {
        dx: f32,
        dy: f32,
    }
    impl Component for Velocity {
        type Storage = PackedStorage<Self>;
    }

    // ── Closure as system ───────────────────────────────────────────────

    #[test]
    fn closure_system() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let mut scheduler = Scheduler::new();
        scheduler.add(|world: &World, _dt: f32| {
            let mut q = world.query_mut::<Health>().unwrap();
            for (_, health) in q.iter_mut() {
                health.0 -= 10.0;
            }
        });

        scheduler.run(&world, 1.0 / 60.0);
        assert_eq!(world.get::<Health>(e).unwrap().0, 90.0);
    }

    // ── Struct implementing System ──────────────────────────────────────

    struct DamageOverTime {
        dps: f32,
    }

    impl System for DamageOverTime {
        fn run(&mut self, world: &World, dt: f32) {
            if let Some(mut q) = world.query_mut::<Health>() {
                for (_, health) in q.iter_mut() {
                    health.0 -= self.dps * dt;
                }
            }
        }

        fn name(&self) -> &str {
            "DamageOverTime"
        }
    }

    #[test]
    fn struct_system() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let mut scheduler = Scheduler::new();
        scheduler.add(DamageOverTime { dps: 60.0 });

        scheduler.run(&world, 0.5);
        assert_eq!(world.get::<Health>(e).unwrap().0, 70.0);
    }

    // ── Stage ordering: systems in different stages run in stage order ──

    #[test]
    fn stages_run_in_order() {
        let order = Arc::new(AtomicU32::new(0));

        let o1 = Arc::clone(&order);
        let o2 = Arc::clone(&order);
        let o3 = Arc::clone(&order);

        let mut scheduler = Scheduler::new();

        scheduler.add_to(Stage::Early, move |_world: &World, _dt: f32| {
            assert_eq!(o1.fetch_add(1, Ordering::SeqCst), 0);
        });
        scheduler.add_to(Stage::Update, move |_world: &World, _dt: f32| {
            assert_eq!(o2.fetch_add(1, Ordering::SeqCst), 1);
        });
        scheduler.add_to(Stage::PostUpdate, move |_world: &World, _dt: f32| {
            assert_eq!(o3.fetch_add(1, Ordering::SeqCst), 2);
        });

        let world = World::new();
        scheduler.run(&world, 0.0);

        assert_eq!(order.load(Ordering::SeqCst), 3);
    }

    // ── Mutation visible across stages ──────────────────────────────────

    #[test]
    fn mutation_visible_across_stages() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 0.0, y: 0.0 });
        world.insert(e, Velocity { dx: 5.0, dy: 10.0 });

        let mut scheduler = Scheduler::new();

        // Stage::Update: apply velocity to position.
        scheduler.add_to(Stage::Update, |world: &World, dt: f32| {
            let (q_vel, mut q_pos) = world.query_2_mut::<Velocity, Position>().unwrap();
            for (entity, vel) in q_vel.iter() {
                if let Some(pos) = q_pos.get_mut(entity) {
                    pos.x += vel.dx * dt;
                    pos.y += vel.dy * dt;
                }
            }
        });

        // Stage::PostUpdate: verify position was updated.
        scheduler.add_to(Stage::PostUpdate, |world: &World, _dt: f32| {
            let q = world.query::<Position>().unwrap();
            let pos = q.get(0).unwrap();
            assert!(pos.x > 0.0, "Update mutation not visible in PostUpdate");
        });

        scheduler.run(&world, 1.0);

        assert_eq!(world.get::<Position>(e).unwrap().x, 5.0);
        assert_eq!(world.get::<Position>(e).unwrap().y, 10.0);
    }

    // ── Parallel within stage: both systems complete ────────────────────

    #[test]
    fn parallel_within_stage() {
        let counter = Arc::new(AtomicU32::new(0));

        let c1 = Arc::clone(&counter);
        let c2 = Arc::clone(&counter);

        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Update, move |_: &World, _: f32| {
            c1.fetch_add(1, Ordering::SeqCst);
        });
        scheduler.add_to(Stage::Update, move |_: &World, _: f32| {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        let world = World::new();
        scheduler.run(&world, 0.0);

        assert_eq!(counter.load(Ordering::SeqCst), 2, "both systems must run");
    }

    // ── Exclusive runs after parallel batch ──────────────────────────────

    #[test]
    fn exclusive_runs_after_parallel() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let mut scheduler = Scheduler::new();

        // Parallel: damage the entity.
        scheduler.add_to(Stage::Late, |world: &World, _dt: f32| {
            let mut q = world.query_mut::<Health>().unwrap();
            for (_, h) in q.iter_mut() {
                h.0 -= 25.0;
            }
        });

        // Exclusive: verify damage was applied (runs after parallel).
        scheduler.add_exclusive(Stage::Late, |world: &World, _dt: f32| {
            let q = world.query::<Health>().unwrap();
            let h = q.get(0).unwrap();
            assert_eq!(h.0, 75.0, "exclusive must see parallel system's writes");
        });

        scheduler.run(&world, 0.0);
    }

    // ── add() defaults to Stage::Update ─────────────────────────────────

    #[test]
    fn add_defaults_to_update() {
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Early, |_: &World, _: f32| {});
        scheduler.add(|_: &World, _: f32| {}); // should land in Update
        scheduler.add_to(Stage::Late, |_: &World, _: f32| {});

        let names = scheduler.system_names();
        assert_eq!(names.len(), 3);
        // The Update system (index 1) should be between Early and Late.
        // We can't check names directly (closures have generated names),
        // but we can verify count and that stages are populated.
        assert!(scheduler.stages.contains_key(&Stage::Early));
        assert!(scheduler.stages.contains_key(&Stage::Update));
        assert!(scheduler.stages.contains_key(&Stage::Late));
    }

    // ── Empty scheduler ─────────────────────────────────────────────────

    #[test]
    fn empty_scheduler_runs_cleanly() {
        let mut scheduler = Scheduler::new();
        let world = World::new();
        scheduler.run(&world, 1.0 / 60.0); // no panic
    }

    // ── Empty intermediate stages are skipped ───────────────────────────

    #[test]
    fn empty_stages_skipped() {
        let counter = Arc::new(AtomicU32::new(0));
        let c1 = Arc::clone(&counter);
        let c2 = Arc::clone(&counter);

        let mut scheduler = Scheduler::new();
        // Only Early and Late — Update/PostUpdate/Physics are empty.
        scheduler.add_to(Stage::Early, move |_: &World, _: f32| {
            c1.fetch_add(1, Ordering::SeqCst);
        });
        scheduler.add_to(Stage::Late, move |_: &World, _: f32| {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        let world = World::new();
        scheduler.run(&world, 0.0);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    // ── system_names preserves stage order ───────────────────────────────

    #[test]
    fn system_names_in_stage_order() {
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Late, DamageOverTime { dps: 10.0 });
        scheduler.add_to(Stage::Early, |_world: &World, _dt: f32| {});

        let names = scheduler.system_names();
        assert_eq!(names.len(), 2);
        // Early system should come first (even though Late was added first).
        // The DamageOverTime struct system is in Late, so it should be second.
        assert_eq!(names[1], "DamageOverTime");
    }

    // ── try_add_* rejects duplicate system names (#312) ─────────────────

    #[test]
    fn try_add_to_rejects_duplicate() {
        // Registering the same-named struct twice via `try_add_to`
        // returns `Err(name)` on the second call and leaves the
        // scheduler with a single entry. The lax `add_to` is retained
        // for closure ergonomics (see #312).
        let mut scheduler = Scheduler::new();
        scheduler
            .try_add_to(Stage::Update, DamageOverTime { dps: 60.0 })
            .ok();
        let result = scheduler.try_add_to(Stage::Update, DamageOverTime { dps: 999.0 });
        match result {
            Err(name) => assert_eq!(name, "DamageOverTime"),
            Ok(_) => panic!("duplicate should be rejected"),
        }
        assert_eq!(scheduler.system_names().len(), 1);
    }

    #[test]
    fn try_add_exclusive_rejects_duplicate() {
        let mut scheduler = Scheduler::new();
        scheduler
            .try_add_exclusive(Stage::Late, DamageOverTime { dps: 10.0 })
            .ok();
        let result = scheduler.try_add_exclusive(Stage::Late, DamageOverTime { dps: 99.0 });
        match result {
            Err(name) => assert_eq!(name, "DamageOverTime"),
            Ok(_) => panic!("duplicate should be rejected"),
        }
        assert_eq!(scheduler.system_names().len(), 1);
    }

    #[test]
    fn try_add_to_rejects_duplicate_across_stages() {
        // Same-named system across two different stages is also a
        // duplicate — the scheduler has a single flat name space.
        let mut scheduler = Scheduler::new();
        scheduler
            .try_add_to(Stage::Early, DamageOverTime { dps: 10.0 })
            .ok();
        let result = scheduler.try_add_to(Stage::Late, DamageOverTime { dps: 99.0 });
        assert!(result.is_err(), "duplicate across stages still rejected");
        assert_eq!(scheduler.system_names().len(), 1);
    }

    #[test]
    fn add_to_still_accepts_duplicate_with_warning() {
        // Closures and intentional re-registration paths still work
        // via the lax `add_to`. This preserves parallel_within_stage,
        // stages_run_in_order, etc. which register multiple closures
        // that happen to share a type_name.
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Update, DamageOverTime { dps: 10.0 });
        scheduler.add_to(Stage::Update, DamageOverTime { dps: 20.0 });
        assert_eq!(scheduler.system_names().len(), 2);
    }

    // ── R7 declared access + conflict report ────────────────────────────

    use super::super::access::ConflictKind;

    /// `DamagePosition` declares a write on Position; pairs with the
    /// closure below that reads Position to exercise the
    /// declared/undeclared and Conflict/Unknown classifications.
    struct DamagePosition;
    impl System for DamagePosition {
        fn run(&mut self, _world: &World, _dt: f32) {}
        fn name(&self) -> &str {
            "DamagePosition"
        }
        fn access(&self) -> Option<Access> {
            Some(Access::new().writes::<Position>())
        }
    }

    /// `ReadPosition` declares a read of Position via
    /// `add_to_with_access` rather than impling `access()` itself.
    /// Mirrors the closure registration path used in the engine binary.
    fn read_position_system(_world: &World, _dt: f32) {}

    #[test]
    fn declared_systems_produce_conflict_in_report() {
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Update, DamagePosition);
        scheduler.add_to_with_access(
            Stage::Update,
            read_position_system,
            Access::new().reads::<Position>(),
        );

        let report = scheduler.access_report();
        assert_eq!(report.system_count(), 2);
        assert_eq!(report.undeclared_count(), 0);
        assert_eq!(report.known_conflict_count(), 1);
        assert_eq!(report.unknown_pair_count(), 0);

        // Drill into the single conflict — must be a write/read pair
        // on Position.
        let stage = report
            .stages
            .iter()
            .find(|s| s.stage == Stage::Update)
            .expect("Update stage in report");
        assert_eq!(stage.conflicts.len(), 1);
        match &stage.conflicts[0].conflict {
            AccessConflict::Conflict { pairs } => {
                assert_eq!(pairs.len(), 1);
                assert!(matches!(
                    pairs[0].kind,
                    ConflictKind::ReadWrite | ConflictKind::WriteRead
                ));
                assert!(pairs[0].type_name.ends_with("Position"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn undeclared_closure_pairs_show_as_unknown() {
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Update, |_w: &World, _dt: f32| {});
        scheduler.add_to(Stage::Update, |_w: &World, _dt: f32| {});

        let report = scheduler.access_report();
        assert_eq!(report.undeclared_count(), 2);
        assert_eq!(report.known_conflict_count(), 0);
        assert_eq!(report.unknown_pair_count(), 1);
        assert!(matches!(
            report.stages[0].conflicts[0].conflict,
            AccessConflict::Unknown { .. }
        ));
    }

    #[test]
    fn declared_disjoint_components_have_no_conflict() {
        struct WriteHealth;
        impl System for WriteHealth {
            fn run(&mut self, _w: &World, _dt: f32) {}
            fn name(&self) -> &str {
                "WriteHealth"
            }
            fn access(&self) -> Option<Access> {
                Some(Access::new().writes::<Health>())
            }
        }
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Update, WriteHealth);
        scheduler.add_to(Stage::Update, DamagePosition);

        let report = scheduler.access_report();
        assert_eq!(report.known_conflict_count(), 0);
        assert_eq!(report.unknown_pair_count(), 0);
    }

    #[test]
    fn override_at_registration_beats_trait_method() {
        // DamagePosition normally writes Position; override at the
        // registration site lets the operator re-declare it as
        // "actually only reads Velocity." Useful when a system
        // hasn't been migrated and we want the operator to assert
        // the access without touching the system source.
        let mut scheduler = Scheduler::new();
        scheduler.add_to_with_access(
            Stage::Update,
            DamagePosition,
            Access::new().reads::<Velocity>(),
        );
        let row = &scheduler.access_report().stages[0].systems[0];
        let declared = row.declared.as_ref().expect("declared");
        assert_eq!(declared.components_read.len(), 1);
        assert_eq!(declared.components_write.len(), 0);
        assert!(declared.components_read[0].type_name.ends_with("Velocity"));
    }

    #[test]
    fn exclusive_systems_are_listed_but_not_paired() {
        // Exclusive systems run after the parallel batch, so they
        // can never collide. They show up in the report's `systems`
        // list but never in `conflicts`.
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Late, DamagePosition);
        scheduler.add_exclusive(Stage::Late, |_w: &World, _dt: f32| {});

        let report = scheduler.access_report();
        let stage = report
            .stages
            .iter()
            .find(|s| s.stage == Stage::Late)
            .unwrap();
        assert_eq!(stage.systems.len(), 2);
        assert!(stage.systems.iter().any(|r| r.is_exclusive));
        // Only one parallel system → no pairs to analyze.
        assert!(stage.conflicts.is_empty());
    }
}
