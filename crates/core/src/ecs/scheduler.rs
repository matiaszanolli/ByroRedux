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
use super::resource::Resource;
use super::system::System;
use super::world::World;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

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

    /// Run the system, recording `(name, ns)` into `timings` only when
    /// the per-system tracker is active (`Some`). When `None` — the
    /// steady-state path, where no `SchedulerSystemTimings` resource
    /// exists — the `Instant::now()` probe and the `Mutex` round-trip
    /// are both skipped entirely (#1647 / #2166).
    fn run_tracked(
        &mut self,
        world: &World,
        dt: f32,
        timings: Option<&Mutex<Vec<(&'static str, u64)>>>,
    ) {
        match timings {
            Some(timings) => {
                // Read the name up front — `name()` borrows
                // `self.system` immutably and would otherwise conflict
                // with the `&mut` run call below. It's `&'static str`,
                // so this is a copy, not an allocation (#2166).
                let name = self.system.name();
                let t0 = Instant::now();
                self.system.run(world, dt);
                let ns = t0.elapsed().as_nanos() as u64;
                timings
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((name, ns));
            }
            None => self.system.run(world, dt),
        }
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
    /// Named ordering constraints, applied by [`Scheduler::resolve_order`] (#6702).
    constraints: Vec<OrderConstraint>,
    /// `true` while a constraint has been added since the last successful
    /// resolve; [`Scheduler::run`] resolves before it runs.
    order_dirty: bool,
}

/// `first` runs before `second` (#6702).
#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderConstraint {
    first: String,
    second: String,
}

/// Why [`Scheduler::resolve_order`] refused a set of constraints. The schedule
/// is left exactly as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleOrderError {
    /// A constraint names a system that is not registered.
    UnknownSystem {
        name: String,
        /// The constraint it appears in, as `first -> second`.
        constraint: String,
    },
    /// The constraint asks for an order the stage and phase order forbids:
    /// stages run in order, and a stage's exclusive phase follows its
    /// parallel phase.
    ContradictsStageOrder { first: String, second: String },
    /// The constraints in one stage phase form a cycle.
    Cycle {
        stage: Stage,
        exclusive: bool,
        /// The systems on the cycle.
        systems: Vec<String>,
    },
}

impl std::fmt::Display for ScheduleOrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSystem { name, constraint } => {
                write!(
                    f,
                    "ordering constraint `{constraint}` names unknown system `{name}`"
                )
            }
            Self::ContradictsStageOrder { first, second } => write!(
                f,
                "ordering constraint `{first} -> {second}` contradicts the stage order \
                 (stages run in order, and a stage's exclusive phase follows its parallel phase)"
            ),
            Self::Cycle {
                stage,
                exclusive,
                systems,
            } => write!(
                f,
                "ordering constraints form a cycle in the {stage:?} stage's {} phase: {}",
                if *exclusive { "exclusive" } else { "parallel" },
                systems.join(", ")
            ),
        }
    }
}

impl std::error::Error for ScheduleOrderError {}

/// The name the scheduler knows `system` by, for [`Scheduler::run_before`] and
/// [`Scheduler::run_after`]. A function item's name is its path, so a subsystem
/// names the systems it orders against without spelling the path as a string.
pub fn system_name<S: System>(system: &S) -> &'static str {
    system.name()
}

/// Where one registered system sits.
#[derive(Clone, Copy)]
struct SystemSlot {
    stage: Stage,
    exclusive: bool,
    index: usize,
}

/// The order of `0..n` that honors `edges` (`(a, b)`: `a` before `b`) and moves
/// as little as it must: systems are placed in registration order, and placing
/// one first places whatever must precede it. A system no constraint touches
/// keeps its place among its neighbours, and with no edges the result is the
/// identity. `Err` holds the indices on the cycle found.
fn stable_order(n: usize, edges: &[(usize, usize)]) -> Result<Vec<usize>, Vec<usize>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unvisited,
        Placing,
        Placed,
    }

    fn place(
        node: usize,
        predecessors: &[Vec<usize>],
        marks: &mut [Mark],
        stack: &mut Vec<usize>,
        order: &mut Vec<usize>,
    ) -> Result<(), Vec<usize>> {
        match marks[node] {
            Mark::Placed => return Ok(()),
            Mark::Placing => {
                let start = stack.iter().position(|&n| n == node).unwrap_or(0);
                return Err(stack[start..].to_vec());
            }
            Mark::Unvisited => {}
        }
        marks[node] = Mark::Placing;
        stack.push(node);
        for &before in &predecessors[node] {
            place(before, predecessors, marks, stack, order)?;
        }
        stack.pop();
        marks[node] = Mark::Placed;
        order.push(node);
        Ok(())
    }

    let mut predecessors = vec![Vec::new(); n];
    for &(before, after) in edges {
        predecessors[after].push(before);
    }
    for list in &mut predecessors {
        list.sort_unstable();
        list.dedup();
    }
    let mut marks = vec![Mark::Unvisited; n];
    let mut order = Vec::with_capacity(n);
    let mut stack = Vec::new();
    for node in 0..n {
        place(node, &predecessors, &mut marks, &mut stack, &mut order)?;
    }
    Ok(order)
}

/// World resource — per-system wall-time of the most recent
/// `Scheduler::run`, sorted descending by milliseconds. Phase 11
/// of the debug-UI plan added this to localize which ECS system
/// dominates the 547 ms `atw_scheduler_ms` budget surfaced at
/// Phase 10. Rayon-parallel systems all report their own wall
/// time; on a stage where N systems run in parallel the sum of
/// reported times exceeds the stage's wall clock by a factor up
/// to N — that's the parallelism gain. A single system reading
/// far above the stage wall time means it dominates and the
/// others run "for free" alongside it.
///
/// # Presence is the gate
///
/// The resource's *existence* arms the tracker — [`Scheduler::run`]
/// probes for it once per frame and skips all per-system timing work
/// when it is absent (#1647). It must therefore only be inserted while
/// something actually consumes it (the egui Metrics panel, or
/// `BYRO_PROFILE=1`); an unconditional insert at world setup silently
/// re-arms every frame of the shipping binary and defeats the gate
/// (PERF-D1-01 / #2166).
#[derive(Default)]
pub struct SchedulerSystemTimings {
    /// `(system_name, ms)` pairs sorted descending. Re-populated
    /// at the end of every `Scheduler::run`.
    ///
    /// `&'static str` rather than `String`: names come from
    /// [`System::name`], which returns a literal or `type_name`. Keeping
    /// them borrowed removes one allocation per system per frame from
    /// the tracked path (#2166); consumers that need ownership (the 2 Hz
    /// `MetricsSnapshot`) convert at their own, far lower, cadence.
    pub systems: Vec<(&'static str, f32)>,
}

impl Resource for SchedulerSystemTimings {}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            stages: BTreeMap::new(),
            constraints: Vec::new(),
            order_dirty: false,
        }
    }

    /// Require that system `first` runs before system `second` (#6702).
    ///
    /// Names are [`System::name`]. Both must sit in the same stage and phase for
    /// the constraint to move anything; a constraint across stages or phases is
    /// either already guaranteed by the stage order or rejected by
    /// [`Scheduler::resolve_order`]. A name shared by several registrations
    /// (closures of one signature) constrains all of them.
    ///
    /// Nothing is reordered until [`Scheduler::resolve_order`] (or the next
    /// [`Scheduler::run`], which calls it). Without constraints, order is
    /// registration order, exactly as before.
    pub fn run_before(&mut self, first: &str, second: &str) -> &mut Self {
        self.constraints.push(OrderConstraint {
            first: first.to_string(),
            second: second.to_string(),
        });
        self.order_dirty = true;
        self
    }

    /// Require that system `second` runs after system `first`: the same
    /// constraint as [`Scheduler::run_before`], spelled from the later system's
    /// side so a subsystem can say what it follows.
    pub fn run_after(&mut self, second: &str, first: &str) -> &mut Self {
        self.run_before(first, second)
    }

    /// Reorder each stage phase to satisfy the constraints added by
    /// [`Scheduler::run_before`] / [`Scheduler::run_after`].
    ///
    /// The sort is stable: a system no constraint touches keeps its place
    /// relative to its neighbours in registration order, and with no constraints
    /// nothing moves. An unknown name, a constraint the stage order forbids, or
    /// a cycle is an error, and the schedule is left unchanged. Calling it
    /// again is harmless.
    pub fn resolve_order(&mut self) -> Result<(), ScheduleOrderError> {
        let mut slots: BTreeMap<&str, Vec<SystemSlot>> = BTreeMap::new();
        for (stage, data) in &self.stages {
            for (exclusive, entries) in [(false, &data.parallel), (true, &data.exclusive)] {
                for (index, entry) in entries.iter().enumerate() {
                    slots
                        .entry(entry.system.name())
                        .or_default()
                        .push(SystemSlot {
                            stage: *stage,
                            exclusive,
                            index,
                        });
                }
            }
        }

        let mut edges: BTreeMap<(Stage, bool), Vec<(usize, usize)>> = BTreeMap::new();
        for constraint in &self.constraints {
            let label = format!("{} -> {}", constraint.first, constraint.second);
            let lookup = |name: &str| {
                slots
                    .get(name)
                    .ok_or_else(|| ScheduleOrderError::UnknownSystem {
                        name: name.to_string(),
                        constraint: label.clone(),
                    })
            };
            let firsts = lookup(&constraint.first)?;
            let seconds = lookup(&constraint.second)?;
            for first in firsts {
                for second in seconds {
                    let (a, b) = (
                        (first.stage, first.exclusive),
                        (second.stage, second.exclusive),
                    );
                    match a.cmp(&b) {
                        std::cmp::Ordering::Less => {}
                        std::cmp::Ordering::Greater => {
                            return Err(ScheduleOrderError::ContradictsStageOrder {
                                first: constraint.first.clone(),
                                second: constraint.second.clone(),
                            });
                        }
                        std::cmp::Ordering::Equal => {
                            edges
                                .entry(a)
                                .or_default()
                                .push((first.index, second.index));
                        }
                    }
                }
            }
        }

        // Plan every phase before touching any, so an error leaves the
        // schedule as it was.
        let mut plans = Vec::new();
        for ((stage, exclusive), phase_edges) in &edges {
            let data = &self.stages[stage];
            let entries = if *exclusive {
                &data.exclusive
            } else {
                &data.parallel
            };
            match stable_order(entries.len(), phase_edges) {
                Ok(order) => plans.push((*stage, *exclusive, order)),
                Err(stuck) => {
                    return Err(ScheduleOrderError::Cycle {
                        stage: *stage,
                        exclusive: *exclusive,
                        systems: stuck
                            .into_iter()
                            .map(|i| entries[i].system.name().to_string())
                            .collect(),
                    });
                }
            }
        }
        for (stage, exclusive, order) in plans {
            let data = self
                .stages
                .get_mut(&stage)
                .expect("planned from this stage");
            let entries = if exclusive {
                &mut data.exclusive
            } else {
                &mut data.parallel
            };
            let mut taken: Vec<Option<SystemEntry>> =
                std::mem::take(entries).into_iter().map(Some).collect();
            *entries = order
                .into_iter()
                .map(|i| {
                    taken[i]
                        .take()
                        .expect("a permutation places each system once")
                })
                .collect();
        }
        self.order_dirty = false;
        Ok(())
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

    /// Add an exclusive system to `stage` with an explicit access
    /// declaration (R7).
    ///
    /// Mirror of [`add_to_with_access`](Self::add_to_with_access) for the
    /// exclusive phase. The conflict analyzer at
    /// [`access_report`](Self::access_report) only walks parallel-stage
    /// pairs today, so a declared access on an exclusive system doesn't
    /// surface new conflict rows — but it *does* take the system out of
    /// the [`AccessReport::undeclared_count`] /
    /// [`AccessReport::undeclared_exclusive_count`] totals, which is the
    /// migration KPI the operator drives toward zero via `sys.accesses`.
    /// Closures and bare `fn(&World, f32)` items have no way to override
    /// `System::access` themselves; this is their declaration channel.
    /// See #1236.
    pub fn add_exclusive_with_access<S: System + 'static>(
        &mut self,
        stage: Stage,
        system: S,
        access: Access,
    ) -> &mut Self {
        let name = system.name().to_string();
        if self.has_system(&name) {
            log::warn!(
                "Scheduler: duplicate exclusive system name '{}' in stage {:?} \
                 (with declared access — see add_exclusive)",
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
                declared_override: Some(access),
            });
        self
    }

    /// Strict sibling of [`add_exclusive_with_access`] — rejects duplicate names.
    /// See #312 / #1236.
    pub fn try_add_exclusive_with_access<S: System + 'static>(
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
            .exclusive
            .push(SystemEntry {
                system: Box::new(system),
                declared_override: Some(access),
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
    ///
    /// ## Policy: fail-fast is intentional (TS-08 / #1412)
    ///
    /// Terminate-on-panic is a **deliberate** choice, not a missing
    /// feature. A panicking system is a bug, and continuing the frame
    /// with half-mutated ECS state risks silent corruption that is far
    /// harder to diagnose than a clean crash at the panic site. The
    /// engine therefore lets the panic abort the process.
    ///
    /// ## Lock poisoning
    ///
    /// Component storages are `std::sync::RwLock` (`World::storages`),
    /// which **poisons** when a thread panics while holding the write
    /// guard. This is already handled defensively: every acquisition
    /// site in [`World`] resolves a `PoisonError` through
    /// `storage_lock_poisoned::<T>()`, which re-panics with a diagnostic
    /// naming the poisoned storage and pointing at the system that
    /// panicked first — so a post-panic access fails loud, never silently
    /// reads torn state.
    ///
    /// Poisoning is also a concrete argument *against* a naive per-frame
    /// `catch_unwind`: the recovered frame would hit
    /// `storage_lock_poisoned` on every storage the failed system was
    /// writing, so "recovery" is not clean. A real recovery scheme would
    /// have to wrap each rayon task body in `catch_unwind`, serialize the
    /// first error to a shared slot, AND clear the poison
    /// (`RwLock::clear_poison`) on the affected storages while accepting
    /// the partial-write — a deliberate, non-trivial design, not a drop-in
    /// guard. Deferred as a medium-term quality item.
    pub fn run(&mut self, world: &World, dt: f32) {
        // #6702 — constraints added since the schedule was last resolved. A bad
        // constraint is a bug in the registration, so fail fast like a panicking
        // system (see the policy above).
        if self.order_dirty {
            if let Err(error) = self.resolve_order() {
                panic!("Scheduler: {error}");
            }
        }
        // Phase 11 — per-system wall-time tracker. Each system
        // pushes `(name, ns)` here as it completes; final list is
        // sorted desc + handed off to the
        // `SchedulerSystemTimings` resource for the debug-UI.
        //
        // The tracker is allocated only when that resource is present
        // (Metrics panel open, or `BYRO_PROFILE=1`). In the steady-state
        // path it's absent, so `timings` stays `None` and `run_tracked`
        // skips the `Instant::now()` probe entirely — no per-frame
        // `Mutex`/`Vec` churn (#1647). Keeping the resource out of the
        // world in that state is what makes the gate real; inserting it
        // unconditionally at world setup defeated it (#2166). When
        // present, cost is one `Instant::now()` + one Mutex-lock-push
        // per system (names are `&'static str`, so no allocation);
        // Mutex contention is bounded by the rayon worker count (≤ 1
        // lock per worker per stage), so a ~40-system schedule pays a
        // few µs at most.
        let timings: Option<Mutex<Vec<(&'static str, u64)>>> = world
            .try_resource::<SchedulerSystemTimings>()
            .is_some()
            .then(|| Mutex::new(Vec::new()));
        for data in self.stages.values_mut() {
            // Phase 1: run parallel systems concurrently.
            #[cfg(feature = "parallel-scheduler")]
            {
                data.parallel
                    .par_iter_mut()
                    .for_each(|entry| entry.run_tracked(world, dt, timings.as_ref()));
            }
            #[cfg(not(feature = "parallel-scheduler"))]
            {
                for entry in &mut data.parallel {
                    entry.run_tracked(world, dt, timings.as_ref());
                }
            }
            // Phase 2: run exclusive systems sequentially.
            for entry in &mut data.exclusive {
                entry.run_tracked(world, dt, timings.as_ref());
            }
        }
        // Sort desc + write to the resource. The tracker is only `Some`
        // when the resource existed at frame start, so the write is
        // unconditional within this arm.
        if let Some(timings) = timings {
            let mut all = timings.into_inner().unwrap_or_else(|e| e.into_inner());
            all.sort_by_key(|e| std::cmp::Reverse(e.1));
            if let Some(mut out) = world.try_resource_mut::<SchedulerSystemTimings>() {
                out.systems.clear();
                const NS_TO_MS: f32 = 1.0e-6;
                out.systems
                    .extend(all.into_iter().map(|(n, ns)| (n, ns as f32 * NS_TO_MS)));
            }
        }
    }

    /// Run a single exclusive system by name, skipping every other
    /// system and every parallel batch.
    ///
    /// #5141 — the engine's native pause menu gates `run` entirely, which
    /// silently stopped `debug_drain_system` with it: debug commands queued
    /// while a menu page was open answered "timeout", then all fired in a
    /// burst when the game resumed. The caller (the engine's paused frame
    /// path) uses this to keep exactly the drain alive while the simulation
    /// is frozen. Returns whether a system with that name was found and
    /// run.
    ///
    /// Per-system timing is skipped on this path (the tracker in `run`
    /// exists to attribute the full frame's stage wall-clock; a lone
    /// paused-mode system has no stage to attribute against).
    pub fn run_exclusive_named(&mut self, world: &World, dt: f32, name: &str) -> bool {
        for data in self.stages.values_mut() {
            for entry in &mut data.exclusive {
                // Read the name before the `&mut` run call — same borrow
                // discipline as `run_tracked`'s tracked arm.
                let matches = entry.system.name() == name;
                if matches {
                    entry.system.run(world, dt);
                    return true;
                }
            }
        }
        false
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
    /// systems undeclared" line in the console command. Counts both
    /// parallel and exclusive phases; see [`undeclared_parallel_count`]
    /// and [`undeclared_exclusive_count`] for the per-phase split (#1237).
    ///
    /// [`undeclared_parallel_count`]: Self::undeclared_parallel_count
    /// [`undeclared_exclusive_count`]: Self::undeclared_exclusive_count
    pub fn undeclared_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|s| s.systems.iter())
            .filter(|row| row.declared.is_none())
            .count()
    }

    /// Subset of [`undeclared_count`](Self::undeclared_count) restricted
    /// to parallel-stage systems. This is the population the conflict
    /// analyzer can actually reason about — driving this to zero closes
    /// out every `Unknown` row in the conflict report. See #1237.
    pub fn undeclared_parallel_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|s| s.systems.iter())
            .filter(|row| !row.is_exclusive && row.declared.is_none())
            .count()
    }

    /// Subset of [`undeclared_count`](Self::undeclared_count) restricted
    /// to exclusive-stage systems. Driving this to zero is purely a
    /// documentation / introspection win — exclusive systems run serially
    /// and the conflict analyzer skips them today — but it gates any
    /// future cross-stage analysis that would want a full picture. See
    /// #1236 + #1237.
    pub fn undeclared_exclusive_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|s| s.systems.iter())
            .filter(|row| row.is_exclusive && row.declared.is_none())
            .count()
    }

    /// How many systems run in a parallel batch (the population the
    /// conflict analyzer can pair at all).
    ///
    /// Distinct from [`system_count`](Self::system_count), which counts
    /// exclusives too. M27's conflict-resolution pattern is monotone
    /// demotion — every conflict found so far was resolved by moving one
    /// side to `add_exclusive` — and the boot guard fails on a conflict
    /// but never on an empty parallel batch, so demotion is always the
    /// cheapest way to a green build. That makes this count, not
    /// `system_count`, the quantity a non-vacuity pin has to floor.
    /// See #2393.
    pub fn parallel_system_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|s| s.systems.iter())
            .filter(|row| !row.is_exclusive)
            .count()
    }

    /// How many same-stage pairs the conflict analyzer actually examined
    /// — `Σ C(parallel_k, 2)` over stages.
    ///
    /// Not derivable from [`stages`](Self::stages)`[..].conflicts`:
    /// [`Scheduler::access_report`] only records rows that are *not*
    /// `AccessConflict::None`, so a fully-clean schedule reports zero
    /// conflict rows whether it analyzed 9 pairs or none at all. This
    /// mirrors the pairing loop in `access_report` — a stage holding one
    /// parallel system contributes 0. See #2393.
    pub fn analyzed_pair_count(&self) -> usize {
        self.stages
            .iter()
            .map(|s| s.systems.iter().filter(|row| !row.is_exclusive).count())
            .map(|k| k * k.saturating_sub(1) / 2)
            .sum()
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

        fn name(&self) -> &'static str {
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

    // ── Per-system timing tracker is gated on the resource (#1647) ──────

    #[test]
    fn system_timings_collected_only_when_resource_present() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let mut scheduler = Scheduler::new();
        scheduler.add(DamageOverTime { dps: 60.0 });

        // No `SchedulerSystemTimings` resource: systems still run, and the
        // run completes without allocating/populating a timing list.
        scheduler.run(&world, 0.5);
        assert_eq!(world.get::<Health>(e).unwrap().0, 70.0);
        assert!(world.try_resource::<SchedulerSystemTimings>().is_none());

        // With the resource present, the tracker populates it.
        world.insert_resource(SchedulerSystemTimings::default());
        scheduler.run(&world, 0.5);
        let timings = world.try_resource::<SchedulerSystemTimings>().unwrap();
        assert_eq!(timings.systems.len(), 1);
        assert_eq!(timings.systems[0].0, "DamageOverTime");
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

    // ── #1238: pin the chain across ALL five stages, not just three ─────

    /// #1238 — sibling of `stages_run_in_order` covering the two stages
    /// the original test left out (`Physics`, `Late`). The `BTreeMap<Stage, _>`
    /// iteration uses `Stage`'s `Ord` derive over the explicit `= N`
    /// discriminants — if a future commit reorders the enum or inserts
    /// a stage between two existing ones, this test catches the drift.
    #[test]
    fn all_five_stages_run_in_order() {
        let order = Arc::new(AtomicU32::new(0));
        let o0 = Arc::clone(&order);
        let o1 = Arc::clone(&order);
        let o2 = Arc::clone(&order);
        let o3 = Arc::clone(&order);
        let o4 = Arc::clone(&order);

        let mut scheduler = Scheduler::new();
        // Register out of stage order so the test verifies the BTreeMap
        // sort, not the registration sequence.
        scheduler.add_to(Stage::Late, move |_w: &World, _dt: f32| {
            assert_eq!(o4.fetch_add(1, Ordering::SeqCst), 4);
        });
        scheduler.add_to(Stage::Early, move |_w: &World, _dt: f32| {
            assert_eq!(o0.fetch_add(1, Ordering::SeqCst), 0);
        });
        scheduler.add_to(Stage::Physics, move |_w: &World, _dt: f32| {
            assert_eq!(o3.fetch_add(1, Ordering::SeqCst), 3);
        });
        scheduler.add_to(Stage::PostUpdate, move |_w: &World, _dt: f32| {
            assert_eq!(o2.fetch_add(1, Ordering::SeqCst), 2);
        });
        scheduler.add_to(Stage::Update, move |_w: &World, _dt: f32| {
            assert_eq!(o1.fetch_add(1, Ordering::SeqCst), 1);
        });

        let world = World::new();
        scheduler.run(&world, 0.0);
        assert_eq!(
            order.load(Ordering::SeqCst),
            5,
            "every stage must run exactly once"
        );
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

    // ── Named ordering constraints (#6702) ───────────────────────────────

    struct Named(&'static str);

    impl System for Named {
        fn run(&mut self, _world: &World, _dt: f32) {}
        fn name(&self) -> &'static str {
            self.0
        }
    }

    struct Logging {
        name: &'static str,
        log: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl System for Logging {
        fn run(&mut self, _world: &World, _dt: f32) {
            self.log.lock().unwrap().push(self.name);
        }
        fn name(&self) -> &'static str {
            self.name
        }
    }

    fn scheduler_of(names: &[&'static str]) -> Scheduler {
        let mut scheduler = Scheduler::new();
        for name in names {
            scheduler.add_to(Stage::Update, Named(name));
        }
        scheduler
    }

    #[test]
    fn no_constraints_keep_registration_order() {
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Late, Named("late_a"));
        scheduler.add_to(Stage::Early, Named("early_a"));
        scheduler.add_to(Stage::Update, Named("c"));
        scheduler.add_to(Stage::Update, Named("a"));
        scheduler.add_to(Stage::Update, Named("b"));
        scheduler.add_exclusive(Stage::Update, Named("x2"));
        scheduler.add_exclusive(Stage::Update, Named("x1"));
        let before: Vec<String> = scheduler
            .system_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        scheduler.resolve_order().unwrap();
        let after: Vec<String> = scheduler
            .system_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(before, after);
        assert_eq!(after, ["early_a", "c", "a", "b", "x2", "x1", "late_a"]);
    }

    #[test]
    fn run_before_moves_the_earlier_system_ahead() {
        let mut scheduler = scheduler_of(&["a", "b", "c"]);
        scheduler.run_before("c", "a");
        scheduler.resolve_order().unwrap();
        assert_eq!(scheduler.system_names(), ["c", "a", "b"]);
    }

    #[test]
    fn run_after_moves_the_later_system_behind() {
        let mut scheduler = scheduler_of(&["a", "b", "c"]);
        scheduler.run_after("b", "c");
        scheduler.resolve_order().unwrap();
        assert_eq!(scheduler.system_names(), ["a", "c", "b"]);
    }

    #[test]
    fn a_satisfied_constraint_moves_nothing() {
        let mut scheduler = scheduler_of(&["a", "b", "c"]);
        scheduler.run_before("a", "c").run_after("c", "b");
        scheduler.resolve_order().unwrap();
        assert_eq!(scheduler.system_names(), ["a", "b", "c"]);
    }

    #[test]
    fn a_chain_of_constraints_orders_transitively() {
        let mut scheduler = scheduler_of(&["a", "b", "c", "d"]);
        scheduler.run_before("d", "c").run_before("c", "a");
        scheduler.resolve_order().unwrap();
        assert_eq!(scheduler.system_names(), ["d", "c", "a", "b"]);
    }

    #[test]
    fn resolving_twice_is_idempotent() {
        let mut scheduler = scheduler_of(&["a", "b", "c"]);
        scheduler.run_before("c", "a");
        scheduler.resolve_order().unwrap();
        scheduler.resolve_order().unwrap();
        assert_eq!(scheduler.system_names(), ["c", "a", "b"]);
    }

    #[test]
    fn an_unknown_name_is_an_error_and_the_schedule_is_untouched() {
        let mut scheduler = scheduler_of(&["a", "b"]);
        scheduler.run_before("b", "a").run_before("ghost", "a");
        let error = scheduler.resolve_order().unwrap_err();
        assert_eq!(
            error,
            ScheduleOrderError::UnknownSystem {
                name: "ghost".into(),
                constraint: "ghost -> a".into(),
            }
        );
        assert_eq!(scheduler.system_names(), ["a", "b"]);
        scheduler.run_after("a", "ghost");
        assert!(matches!(
            scheduler.resolve_order(),
            Err(ScheduleOrderError::UnknownSystem { .. })
        ));
    }

    #[test]
    fn a_cycle_is_an_error_and_the_schedule_is_untouched() {
        let mut scheduler = scheduler_of(&["a", "b", "c"]);
        scheduler
            .run_before("c", "a")
            .run_before("a", "b")
            .run_before("b", "a");
        match scheduler.resolve_order().unwrap_err() {
            ScheduleOrderError::Cycle {
                stage,
                exclusive,
                systems,
            } => {
                assert_eq!(stage, Stage::Update);
                assert!(!exclusive);
                assert!(systems.contains(&"a".to_string()) && systems.contains(&"b".to_string()));
            }
            other => panic!("expected a cycle, got {other:?}"),
        }
        assert_eq!(scheduler.system_names(), ["a", "b", "c"]);
    }

    #[test]
    fn a_system_ordered_before_itself_is_a_cycle() {
        let mut scheduler = scheduler_of(&["a", "b"]);
        scheduler.run_before("a", "a");
        assert!(matches!(
            scheduler.resolve_order(),
            Err(ScheduleOrderError::Cycle { .. })
        ));
    }

    #[test]
    fn constraints_across_stages_and_phases_are_checked_against_the_stage_order() {
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Early, Named("early"));
        scheduler.add_to(Stage::Late, Named("late"));
        scheduler.add_to(Stage::Late, Named("late_parallel"));
        scheduler.add_exclusive(Stage::Late, Named("late_exclusive"));

        // Already guaranteed: nothing moves.
        scheduler.run_before("early", "late");
        scheduler.run_before("late_parallel", "late_exclusive");
        scheduler.resolve_order().unwrap();
        assert_eq!(
            scheduler.system_names(),
            ["early", "late", "late_parallel", "late_exclusive"]
        );

        // Forbidden: a later stage cannot run first.
        scheduler.run_before("late", "early");
        assert_eq!(
            scheduler.resolve_order().unwrap_err(),
            ScheduleOrderError::ContradictsStageOrder {
                first: "late".into(),
                second: "early".into(),
            }
        );
    }

    #[test]
    fn the_exclusive_phase_is_ordered_and_runs_in_that_order() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut scheduler = Scheduler::new();
        for name in ["first", "second", "third"] {
            scheduler.add_exclusive(
                Stage::Late,
                Logging {
                    name,
                    log: log.clone(),
                },
            );
        }
        scheduler.run_after("first", "third");
        scheduler.run(&World::new(), 0.0);
        assert_eq!(*log.lock().unwrap(), ["third", "first", "second"]);
    }

    #[test]
    fn run_resolves_pending_constraints_and_fails_fast_on_a_bad_one() {
        let result = std::panic::catch_unwind(|| {
            let mut scheduler = scheduler_of(&["a"]);
            scheduler.run_before("ghost", "a");
            scheduler.run(&World::new(), 0.0);
        });
        assert!(
            result.is_err(),
            "an unresolvable constraint must not be skipped"
        );
    }

    #[test]
    fn constraints_keep_declared_access_attached_to_the_moved_system() {
        let mut scheduler = Scheduler::new();
        scheduler.add_to_with_access(
            Stage::Update,
            Named("reader"),
            Access::new().reads::<Health>(),
        );
        scheduler.add_to_with_access(
            Stage::Update,
            Named("writer"),
            Access::new().writes::<Health>(),
        );
        scheduler.run_before("writer", "reader");
        scheduler.resolve_order().unwrap();
        let report = scheduler.access_report();
        let rows = &report.stages[0].systems;
        assert_eq!(rows[0].name, "writer");
        assert_eq!(rows[0].declared.as_ref().unwrap().components_write.len(), 1);
        assert_eq!(rows[1].name, "reader");
        assert_eq!(rows[1].declared.as_ref().unwrap().components_read.len(), 1);
    }

    // ── run_exclusive_named: the paused-mode drain path (#5141) ──────────

    struct CountingSystem {
        runs: Arc<AtomicU32>,
        name: &'static str,
    }

    impl System for CountingSystem {
        fn run(&mut self, _world: &World, _dt: f32) {
            self.runs.fetch_add(1, Ordering::SeqCst);
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[test]
    fn run_exclusive_named_runs_only_the_named_system() {
        let world = World::new();
        let drain_runs = Arc::new(AtomicU32::new(0));
        let other_exclusive_runs = Arc::new(AtomicU32::new(0));
        let parallel_runs = Arc::new(AtomicU32::new(0));

        let mut scheduler = Scheduler::new();
        scheduler.add_to(
            Stage::Late,
            CountingSystem {
                runs: Arc::clone(&parallel_runs),
                name: "parallel",
            },
        );
        scheduler.add_exclusive(
            Stage::Late,
            CountingSystem {
                runs: Arc::clone(&other_exclusive_runs),
                name: "other_exclusive",
            },
        );
        scheduler.add_exclusive(
            Stage::Late,
            CountingSystem {
                runs: Arc::clone(&drain_runs),
                name: "debug_drain_system",
            },
        );

        assert!(scheduler.run_exclusive_named(&world, 0.0, "debug_drain_system"));
        assert_eq!(
            drain_runs.load(Ordering::SeqCst),
            1,
            "the named system must run exactly once per call"
        );
        assert_eq!(
            other_exclusive_runs.load(Ordering::SeqCst),
            0,
            "other exclusive systems must stay gated"
        );
        assert_eq!(
            parallel_runs.load(Ordering::SeqCst),
            0,
            "parallel batches must stay gated"
        );

        // Unknown names report a miss and run nothing.
        assert!(!scheduler.run_exclusive_named(&world, 0.0, "no_such_system"));
        assert_eq!(drain_runs.load(Ordering::SeqCst), 1);
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
        fn name(&self) -> &'static str {
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
            fn name(&self) -> &'static str {
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

    // ── #1236: add_exclusive_with_access lets closures/fns declare ──────

    /// #1236 — `add_exclusive_with_access` attaches a `declared_override`
    /// on the exclusive vector so closure / bare-fn exclusives can carry
    /// access metadata that the blanket Fn impl can't surface via
    /// `System::access`. Verifies the report row shows `declared = Some(_)`
    /// and that `undeclared_count` decrements accordingly.
    #[test]
    fn add_exclusive_with_access_attaches_declaration() {
        let mut scheduler = Scheduler::new();
        // Plain `add_exclusive` — declared stays None.
        scheduler.add_exclusive(Stage::Late, |_w: &World, _dt: f32| {});
        // _with_access variant — declared lands on the row.
        scheduler.add_exclusive_with_access(
            Stage::Late,
            |_w: &World, _dt: f32| {},
            Access::new().writes::<Position>(),
        );

        let report = scheduler.access_report();
        assert_eq!(report.system_count(), 2);
        assert_eq!(
            report.undeclared_count(),
            1,
            "the plain add_exclusive stays undeclared; the _with_access one declares"
        );

        let stage = &report.stages[0];
        let declared_rows: Vec<_> = stage
            .systems
            .iter()
            .filter(|r| r.declared.is_some())
            .collect();
        assert_eq!(declared_rows.len(), 1);
        assert!(declared_rows[0].is_exclusive);
        let declared = declared_rows[0].declared.as_ref().unwrap();
        assert_eq!(declared.components_write.len(), 1);
        assert!(declared.components_write[0].type_name.ends_with("Position"));
    }

    /// #1236 — `try_add_exclusive_with_access` rejects duplicates by
    /// name, mirroring `try_add_to_with_access` on the parallel side
    /// and `try_add_exclusive` on the lax-exclusive side.
    #[test]
    fn try_add_exclusive_with_access_rejects_duplicate() {
        let mut scheduler = Scheduler::new();
        scheduler
            .try_add_exclusive_with_access(
                Stage::Late,
                DamagePosition,
                Access::new().writes::<Position>(),
            )
            .ok();
        let result = scheduler.try_add_exclusive_with_access(
            Stage::Late,
            DamagePosition,
            Access::new().writes::<Position>(),
        );
        match result {
            Err(name) => assert_eq!(name, "DamagePosition"),
            Ok(_) => panic!("duplicate exclusive _with_access should be rejected"),
        }
        assert_eq!(scheduler.system_names().len(), 1);
    }

    // ── #1237: per-phase undeclared split ───────────────────────────────

    /// #1237 — `undeclared_parallel_count` + `undeclared_exclusive_count`
    /// partition the union `undeclared_count` by phase so the operator
    /// can tell apart "parallel-stage systems not yet migrated" from
    /// "exclusive-stage systems not yet declared." Drives the `sys.accesses`
    /// console breakdown.
    #[test]
    fn undeclared_count_splits_into_parallel_and_exclusive() {
        let mut scheduler = Scheduler::new();
        // Two parallel — one declared, one not.
        scheduler.add_to(Stage::Update, |_w: &World, _dt: f32| {});
        scheduler.add_to_with_access(
            Stage::Update,
            DamagePosition,
            Access::new().writes::<Position>(),
        );
        // Three exclusive — two undeclared (lax-add) + one declared.
        scheduler.add_exclusive(Stage::Late, |_w: &World, _dt: f32| {});
        scheduler.add_exclusive(Stage::Late, |_w: &World, _dt: f32| {});
        scheduler.add_exclusive_with_access(
            Stage::Late,
            |_w: &World, _dt: f32| {},
            Access::new().reads::<Position>(),
        );

        let report = scheduler.access_report();
        assert_eq!(report.system_count(), 5);
        // Union: 1 parallel undeclared + 2 exclusive undeclared = 3.
        assert_eq!(report.undeclared_count(), 3);
        assert_eq!(
            report.undeclared_parallel_count(),
            1,
            "only the lax parallel closure stays undeclared"
        );
        assert_eq!(
            report.undeclared_exclusive_count(),
            2,
            "the two lax exclusive closures stay undeclared"
        );
        // Round-trip invariant: the split always sums to the union.
        assert_eq!(
            report.undeclared_parallel_count() + report.undeclared_exclusive_count(),
            report.undeclared_count(),
        );
    }
}
