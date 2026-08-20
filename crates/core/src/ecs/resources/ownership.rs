//! Cross-subsystem ownership accounting for the exterior soak gate.
//!
//! EX-08 (#2374) requires proof that repeated out-and-back traversal returns
//! every CPU/GPU/runtime owner to its pre-entry baseline. The owners live in
//! six different crates — ECS rows here, GPU registries in `byroredux-renderer`,
//! bodies in `byroredux-physics`, tracks in `byroredux-audio` — so no single
//! crate can observe them all.
//!
//! This module is the *shape* of that observation, not its collection: a plain
//! data resource in the same spirit as [`DebugStats`](super::DebugStats), which
//! the binary fills each sample tick from the subsystems it owns. Keeping the
//! accounting rules here (rather than in the binary) is what makes them
//! testable without a Vulkan device or game data — the live soak then only has
//! to supply numbers, not judgement.
//!
//! # Why a per-class reclaim policy
//!
//! "Returns to baseline" is false for several owners *by design*, and a gate
//! that ignored that would either fail every run or be silenced into
//! uselessness:
//!
//! - The mesh and texture registries never reuse handles. A dropped slot stays
//!   as a placeholder so a dangling `GpuInstance.mesh_id` / `texture_index` can
//!   never reappear pointing at a *different* resource (#372). Registry length
//!   is therefore monotonic on purpose.
//! - `EntityId` allocation is likewise monotonic (see `World::despawn`).
//! - `SoundCache` is an explicit retain-across-cells cache.
//!
//! Each class declares a [`ReclaimPolicy`] so the gate can hold the reclaimable
//! owners to an exact return while still catching *unbounded* growth in the
//! ones that legitimately retain. This is the "modulo documented bounded
//! caches" clause of EX-08 made executable.

use super::Resource;

/// What a class is expected to do across a full load → unload cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReclaimPolicy {
    /// Must come back to the baseline value. Any steady-state surplus is a
    /// leaked owner.
    Exact,
    /// May retain across cycles (a documented cache), but must not climb
    /// without bound. Only the monotonic-growth check applies.
    Bounded,
    /// An allocator watermark that rises by design and can never fall.
    /// Reported for context; never failed on.
    ///
    /// Three counters are in this class, and all three are monotonic for the
    /// *same* reason — an identity, once issued, is never reissued:
    ///
    /// - `entities_spawned` — `EntityId`s grow monotonically so a stale id can
    ///   never address a live entity (see `World::despawn`).
    /// - `meshes_registry` / `textures_registry` — these are slot-vector
    ///   *lengths*. A dropped mesh or texture frees its GPU memory but leaves
    ///   a placeholder slot behind, so a dangling `GpuInstance.mesh_id` /
    ///   `texture_index` can never resolve to a *different* resource (#372).
    ///
    /// Under repeated streaming these therefore climb forever by construction:
    /// re-entering a cell issues fresh handles for the same content. Failing on
    /// that would fail every correct run. The residency question they look like
    /// they answer is answered instead by `meshes_live_slots` /
    /// `texture_live_slots`, which are `Exact` and do fall on unload.
    Monotonic,
}

/// A live count of every owner class EX-08 names, sampled at one instant.
///
/// Counts are `u64` rather than `usize` so a snapshot serialises identically
/// across host word sizes — the soak harness diffs these as text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnershipSnapshot {
    // ── ECS ─────────────────────────────────────────────
    /// `World::next_entity_id()` — total ever spawned, never decreases.
    pub entities_spawned: u64,
    /// Live `Transform` rows. The closest honest stand-in for "live entities":
    /// the ECS has no separate liveness table, and essentially every spawned
    /// entity carries a transform.
    pub transform_rows: u64,
    /// Live `CellRoot` rows — one per entity owned by any resident cell.
    pub cell_root_rows: u64,
    /// `CellRootIndex` map entries — one per *resident cell*, not per entity.
    /// Drained by `unload_cell_inner`; a surplus here means a cell root
    /// outlived its unload.
    pub cell_root_index_entries: u64,
    /// Live `PrecombinedMesh` rows — entities spawned from a baked FO4
    /// `_oc.nif` precombine object, a subset of `cell_root_rows` split out
    /// on its own (EX-15 / #2369) so a leak specific to precombine
    /// geometry — as opposed to ordinary per-REFR architecture — surfaces
    /// as its own named finding instead of hiding inside the aggregate.
    pub precombine_mesh_rows: u64,

    // ── GPU ─────────────────────────────────────────────
    /// Distinct non-zero `MeshHandle` values reachable from live entities.
    ///
    /// A *reachability* count, not a residency one — see the `Bounded`
    /// classification in [`OwnershipSnapshot::classes`] for why it cannot be
    /// held to an exact return.
    pub meshes_in_use: u64,
    /// `MeshRegistry::len()` — the slot-vector length, monotonic by the #372
    /// handle-retirement policy. A watermark, not a residency figure.
    pub meshes_registry: u64,
    /// Occupied mesh slots. The residency counterpart of `meshes_registry`.
    pub meshes_live_slots: u64,
    /// Distinct non-zero `TextureHandle` values reachable from live entities.
    pub textures_in_use: u64,
    /// `TextureRegistry::len()` — slot-vector length, monotonic, same policy.
    pub textures_registry: u64,
    /// Texture slots the registry still counts as live (refcount > 0).
    pub texture_live_slots: u64,
    /// Populated `BlasEntry` slots. The backing `Vec` is index-addressed by
    /// mesh handle so its *length* is monotonic; this counts `Some`.
    pub blas_entries: u64,
    /// TLAS instance records written for the most recent build.
    pub tlas_instances: u64,
    /// Allocated terrain tile slots.
    pub terrain_tiles: u64,

    // ── Runtime ─────────────────────────────────────────
    /// Rapier rigid bodies in the solver set.
    pub physics_bodies: u64,
    /// Sounds kira is still playing or fading.
    pub audio_active_sounds: u64,
    /// Queued fire-and-forget one-shots not yet dispatched.
    pub audio_pending_oneshots: u64,
    /// `SoundCache` entries — a documented retain-across-cells cache.
    pub sound_cache_entries: u64,
    /// Live `ScriptVariables` rows (per-entity Papyrus VM state).
    pub script_variable_rows: u64,
    /// Live `ScriptTimer` rows.
    pub script_timer_rows: u64,
    /// Live `ParticleEmitter` rows.
    pub particle_emitters: u64,
    /// Live water-plane owners.
    pub water_planes: u64,
}

/// One owner class: display name, sampled value, and reclaim expectation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerClass {
    pub name: &'static str,
    pub value: u64,
    pub policy: ReclaimPolicy,
}

impl OwnershipSnapshot {
    /// Every class in a stable order, so two snapshots zip positionally.
    ///
    /// Adding a field to [`OwnershipSnapshot`] without adding it here would
    /// silently drop it from the gate; `ownership_classes_cover_every_field`
    /// pins the count.
    pub fn classes(&self) -> Vec<OwnerClass> {
        use ReclaimPolicy::{Bounded, Exact, Monotonic};
        vec![
            OwnerClass {
                name: "entities_spawned",
                value: self.entities_spawned,
                policy: Monotonic,
            },
            OwnerClass {
                name: "transform_rows",
                value: self.transform_rows,
                policy: Exact,
            },
            OwnerClass {
                name: "cell_root_rows",
                value: self.cell_root_rows,
                policy: Exact,
            },
            OwnerClass {
                name: "cell_root_index_entries",
                value: self.cell_root_index_entries,
                policy: Exact,
            },
            OwnerClass {
                name: "precombine_mesh_rows",
                value: self.precombine_mesh_rows,
                policy: Exact,
            },
            // `meshes_in_use` / `textures_in_use` count *distinct handle
            // values* reachable from live entities, which is a different
            // question from "how much is resident". Under the #372
            // never-reuse policy, re-entering a cell issues fresh handles for
            // content whose old slots were freed while reusing the handles of
            // content that stayed resident, so the same scene legitimately
            // maps to a different handle *set* each visit.
            //
            // The FNV WastelandNV soak measured exactly that: across five
            // settled cycles at the same position, with `transform_rows`
            // (3733), `cell_root_rows` (4556), `meshes_live_slots` (718),
            // `blas_entries` (494) and `physics_bodies` (874) all pinned to a
            // constant, `meshes_in_use` oscillated 620/715/591/620/715 —
            // always at or below the flat live-slot count, and never
            // monotonic. Holding these to an exact return would fail a clean
            // engine on a phase coincidence.
            //
            // They keep the growth check, which is the part that would still
            // catch a real leak: a genuine one climbs, it does not oscillate.
            // The exact-return duty belongs to `meshes_live_slots` /
            // `texture_live_slots`, which do fall on unload.
            OwnerClass {
                name: "meshes_in_use",
                value: self.meshes_in_use,
                policy: Bounded,
            },
            OwnerClass {
                name: "meshes_registry",
                value: self.meshes_registry,
                policy: Monotonic,
            },
            OwnerClass {
                name: "meshes_live_slots",
                value: self.meshes_live_slots,
                policy: Exact,
            },
            OwnerClass {
                name: "textures_in_use",
                value: self.textures_in_use,
                policy: Bounded,
            },
            OwnerClass {
                name: "textures_registry",
                value: self.textures_registry,
                policy: Monotonic,
            },
            OwnerClass {
                name: "texture_live_slots",
                value: self.texture_live_slots,
                policy: Exact,
            },
            OwnerClass {
                name: "blas_entries",
                value: self.blas_entries,
                policy: Exact,
            },
            OwnerClass {
                name: "tlas_instances",
                value: self.tlas_instances,
                policy: Exact,
            },
            OwnerClass {
                name: "terrain_tiles",
                value: self.terrain_tiles,
                policy: Exact,
            },
            OwnerClass {
                name: "physics_bodies",
                value: self.physics_bodies,
                policy: Exact,
            },
            OwnerClass {
                name: "audio_active_sounds",
                value: self.audio_active_sounds,
                policy: Exact,
            },
            OwnerClass {
                name: "audio_pending_oneshots",
                value: self.audio_pending_oneshots,
                policy: Exact,
            },
            OwnerClass {
                name: "sound_cache_entries",
                value: self.sound_cache_entries,
                policy: Bounded,
            },
            OwnerClass {
                name: "script_variable_rows",
                value: self.script_variable_rows,
                policy: Exact,
            },
            OwnerClass {
                name: "script_timer_rows",
                value: self.script_timer_rows,
                policy: Exact,
            },
            OwnerClass {
                name: "particle_emitters",
                value: self.particle_emitters,
                policy: Exact,
            },
            OwnerClass {
                name: "water_planes",
                value: self.water_planes,
                policy: Exact,
            },
        ]
    }

    /// Element-wise maximum — used to fold a running high-water mark.
    pub fn max_with(&self, other: &Self) -> Self {
        let a = self.classes();
        let b = other.classes();
        let mut out = *self;
        // Rebuild from the zipped max rather than 20 hand-written `.max()`
        // lines, which is where a copy-paste field mix-up would hide.
        let maxed: Vec<u64> = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| x.value.max(y.value))
            .collect();
        out.write_values(&maxed);
        out
    }

    /// Overwrite every field from a positional value list in [`Self::classes`]
    /// order. Private helper for [`Self::max_with`]; the length is asserted so
    /// a field added to one list but not the other fails loudly.
    fn write_values(&mut self, v: &[u64]) {
        assert_eq!(
            v.len(),
            self.classes().len(),
            "value list must match class count"
        );
        let mut i = 0;
        let mut next = || {
            let x = v[i];
            i += 1;
            x
        };
        self.entities_spawned = next();
        self.transform_rows = next();
        self.cell_root_rows = next();
        self.cell_root_index_entries = next();
        self.precombine_mesh_rows = next();
        self.meshes_in_use = next();
        self.meshes_registry = next();
        self.meshes_live_slots = next();
        self.textures_in_use = next();
        self.textures_registry = next();
        self.texture_live_slots = next();
        self.blas_entries = next();
        self.tlas_instances = next();
        self.terrain_tiles = next();
        self.physics_bodies = next();
        self.audio_active_sounds = next();
        self.audio_pending_oneshots = next();
        self.sound_cache_entries = next();
        self.script_variable_rows = next();
        self.script_timer_rows = next();
        self.particle_emitters = next();
        self.water_planes = next();
    }
}

/// A single gate failure: one owner class that did not behave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnershipFinding {
    pub class: &'static str,
    pub kind: FindingKind,
    /// Human-readable evidence — baseline/final values or the growth series.
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingKind {
    /// An `Exact` class ended a cycle above its baseline.
    NotReclaimed,
    /// Any class rose in every consecutive cycle — unbounded growth.
    MonotonicGrowth,
}

/// Baseline, per-cycle samples, and high-water marks for one soak run.
///
/// The tracker is deliberately ignorant of *how* a cycle is driven (traversal,
/// door transition, save/load); it only needs a snapshot per completed cycle.
#[derive(Clone, Debug, Default)]
pub struct OwnershipTracker {
    baseline: Option<OwnershipSnapshot>,
    cycles: Vec<OwnershipSnapshot>,
    high_water: OwnershipSnapshot,
}

/// Minimum completed cycles before monotonic growth is a meaningful claim.
///
/// Two rising samples are just a load; three consecutive rises across
/// *settled* cycles is a trend. Set here rather than at the call site so the
/// unit tests and the live gate cannot disagree.
pub const MIN_CYCLES_FOR_GROWTH: usize = 4;

impl OwnershipTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the pre-entry state. The soak takes this *after* the first cell
    /// has settled and then unloaded, so one-time bootstrap allocations
    /// (worldspace weather textures, the fallback checkerboard, the reverb send
    /// track) are already inside the baseline rather than counted as leaks.
    pub fn set_baseline(&mut self, snapshot: OwnershipSnapshot) {
        self.high_water = self.high_water.max_with(&snapshot);
        self.baseline = Some(snapshot);
    }

    pub fn baseline(&self) -> Option<&OwnershipSnapshot> {
        self.baseline.as_ref()
    }

    pub fn high_water(&self) -> &OwnershipSnapshot {
        &self.high_water
    }

    pub fn cycles(&self) -> &[OwnershipSnapshot] {
        &self.cycles
    }

    /// Record one completed out-and-back cycle, folding the high-water mark.
    pub fn record_cycle(&mut self, snapshot: OwnershipSnapshot) {
        self.high_water = self.high_water.max_with(&snapshot);
        self.cycles.push(snapshot);
    }

    /// Evaluate the EX-08 gate over everything recorded so far.
    ///
    /// Empty result == pass. Two independent checks run:
    ///
    /// 1. **Not reclaimed** — the *last* cycle of an `Exact` class sits above
    ///    baseline. Uses the last cycle rather than every cycle because a
    ///    mid-run sample can legitimately be high (hysteresis has not yet
    ///    evicted); only the settled end state is a leak claim.
    /// 2. **Monotonic growth** — a class rose across every consecutive pair.
    ///    Applies to `Bounded` classes too: a cache that never stops growing is
    ///    not bounded.
    pub fn evaluate(&self) -> Vec<OwnershipFinding> {
        let mut findings = Vec::new();
        let Some(baseline) = self.baseline else {
            return findings;
        };
        let Some(last) = self.cycles.last() else {
            return findings;
        };

        let base_classes = baseline.classes();
        let last_classes = last.classes();
        for (b, l) in base_classes.iter().zip(last_classes.iter()) {
            if l.policy == ReclaimPolicy::Exact && l.value > b.value {
                findings.push(OwnershipFinding {
                    class: l.name,
                    kind: FindingKind::NotReclaimed,
                    detail: format!(
                        "baseline {} → {} after {} cycle(s) (+{})",
                        b.value,
                        l.value,
                        self.cycles.len(),
                        l.value - b.value
                    ),
                });
            }
        }

        if self.cycles.len() >= MIN_CYCLES_FOR_GROWTH {
            let series: Vec<Vec<OwnerClass>> = self.cycles.iter().map(|c| c.classes()).collect();
            let class_count = series[0].len();
            for idx in 0..class_count {
                if series[0][idx].policy == ReclaimPolicy::Monotonic {
                    continue;
                }
                let values: Vec<u64> = series.iter().map(|c| c[idx].value).collect();
                if values.windows(2).all(|w| w[1] > w[0]) {
                    findings.push(OwnershipFinding {
                        class: series[0][idx].name,
                        kind: FindingKind::MonotonicGrowth,
                        detail: format!(
                            "strictly increasing across {} cycles: {:?}",
                            values.len(),
                            values
                        ),
                    });
                }
            }
        }

        findings
    }

    /// Render the full report the soak harness prints and the smoke script
    /// greps. Stable, line-oriented, and greppable by class name.
    pub fn report(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let Some(baseline) = self.baseline else {
            lines.push("ownership: no baseline recorded".to_string());
            return lines;
        };
        let last = self.cycles.last().copied().unwrap_or(baseline);
        lines.push(format!(
            "ownership: {} cycle(s) recorded",
            self.cycles.len()
        ));
        lines.push(format!(
            "  {:<26} {:>10} {:>10} {:>10} {:>8}",
            "class", "baseline", "final", "highwater", "policy"
        ));
        let hw = self.high_water.classes();
        for ((b, l), h) in baseline
            .classes()
            .iter()
            .zip(last.classes().iter())
            .zip(hw.iter())
        {
            let policy = match l.policy {
                ReclaimPolicy::Exact => "exact",
                ReclaimPolicy::Bounded => "bounded",
                ReclaimPolicy::Monotonic => "monotonic",
            };
            let flag = if l.policy == ReclaimPolicy::Exact && l.value > b.value {
                " LEAK"
            } else {
                ""
            };
            lines.push(format!(
                "  {:<26} {:>10} {:>10} {:>10} {:>8}{}",
                l.name, b.value, l.value, h.value, policy, flag
            ));
        }
        // Per-cycle series for every class. A surplus that *saturates* (rises
        // once, then holds) is a different animal from one that keeps climbing,
        // and baseline/final/high-water alone cannot tell them apart — which is
        // exactly the judgement an operator has to make when triaging a
        // NOT-RECLAIMED finding.
        if !self.cycles.is_empty() {
            lines.push(String::new());
            lines.push("  per-cycle series:".to_string());
            let per_cycle: Vec<Vec<OwnerClass>> = self.cycles.iter().map(|c| c.classes()).collect();
            for (idx, class) in baseline.classes().iter().enumerate() {
                let values: Vec<String> =
                    per_cycle.iter().map(|c| c[idx].value.to_string()).collect();
                lines.push(format!("    {:<26} {}", class.name, values.join(" ")));
            }
        }

        let findings = self.evaluate();
        if findings.is_empty() {
            lines.push("ownership: PASS".to_string());
        } else {
            for f in &findings {
                let kind = match f.kind {
                    FindingKind::NotReclaimed => "NOT-RECLAIMED",
                    FindingKind::MonotonicGrowth => "MONOTONIC-GROWTH",
                };
                lines.push(format!(
                    "ownership: FAIL {} {} — {}",
                    kind, f.class, f.detail
                ));
            }
        }
        lines
    }
}

impl Resource for OwnershipTracker {}

/// Latest sampled snapshot, refreshed by the binary's telemetry tick.
///
/// Split from [`OwnershipTracker`] so the per-frame sampler can write a cheap
/// value type without touching the tracker's history, and so `world.owners`
/// can read the current state whether or not a soak is running.
#[derive(Clone, Copy, Debug, Default)]
pub struct OwnershipTelemetry {
    pub current: OwnershipSnapshot,
}

impl Resource for OwnershipTelemetry {}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;
