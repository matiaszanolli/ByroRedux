# Issue #3121: physics_sync_system's WATAL buoyancy phase reads TotalTime, WindField and WaterCurrentVolume undeclared (+ make_animation_system reads Children undeclared)

- **Finding ID**: `CONC-2026-08-20-02`
- **Severity**: MEDIUM
- **Labels**: `medium,sync,bug`
- **Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3121

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3121 --json state`.

---

**Severity**: MEDIUM
**Dimension**: Scheduler Access Declarations (regression guard)
**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md` (`CONC-2026-08-20-02`), independently
corroborated by `docs/audits/AUDIT_ECS_2026-08-20.md` (`ECS-2026-08-20-04`), which found the same
`physics_sync_system` gap **plus a fourth, in `make_animation_system` — folded in below**. Filed once,
from CONCURRENCY.

**Location**
- `byroredux/src/boot.rs:1233-1290` — the `physics_sync_system` `Access` chain (the gap)
- `crates/physics/src/water.rs:476` (`TotalTime`), `:478` and `:325` (`WindField`), `:378-383`
  (`WaterCurrentVolume`) — the undeclared reads
- `byroredux/src/boot.rs:950-982` — the `make_animation_system()` `Access` chain (the fourth gap)
- `byroredux/src/systems/animation.rs:65` -> `byroredux/src/anim_convert.rs:29` — the undeclared
  `Children` read

## Description

### `physics_sync_system` — three undeclared reads

The WATAL buoyancy phase added in this delta (`apply_buoyancy`, `crates/physics/src/water.rs:465`) reads
three things the `physics_sync_system` `Access` chain does not name. Two are resources (`TotalTime` at
`:476`, `WindField` at `:478` and again transitively through `weather_wave_adjustment` at `:325`); one is
a component storage (`WaterCurrentVolume`, a real `SparseSetStorage` component —
`crates/core/src/ecs/components/water.rs:510-517` — read by `collect_water_current_volumes` at
`:378-383`).

The declaration was clearly extended for WATAL in this same delta: it gained `PhysicsWaterConstants`,
`WaterPlane`, `WaterVolume`, `WaterFlow` and `WaterContact`, each with a comment explaining why. The
three above were missed. `WaterCurrentVolume` is the easiest to miss because the placed-XWCU
current-volume path is a separate collector from the water-surface one; `WindField` is missed for the
same transitive-call-frame reason as #3111.

### `make_animation_system()` — one undeclared read (from the ECS-side report)

`make_animation_system()` (`add_to_with_access(Stage::Update, …)`) reads `Children` via
`build_subtree_name_map` on the subtree-cache miss path. Not declared. Pre-existing rather than
delta-introduced, but not covered by any prior report. The chain declares `AnimationClipRegistry`,
`SubtreeCache`, `NameIndex`, `StringPool`, `Name` and ~15 component writes — no `Children`.

### Why MEDIUM and not HIGH

There is no parallel counterparty today for either system, so nothing races and nothing is
non-deterministic. `Stage::Physics` holds exactly one system
(`grep -n "Stage::Physics" byroredux/src/boot.rs` -> one registration at `:1235`), `make_animation_system()`
is the only member of `Stage::Update`'s parallel batch, and stages run sequentially. Every pair the
analyzer *could* form is empty.

The damage is that the declaration is the contract a *future* same-stage system is analysed against, and
the analyzer would clear a wind-writing, clock-reading or current-volume-writing sibling as
conflict-free. That is precisely the argument the codebase itself makes in the `#1787 / CONC-D4-01`
comment already sitting six lines above the gap:

> must be declared so a future parallel system that writes it is caught by the conflict analyzer instead
> of silently racing

Four quiet omissions in one delta is the drift rate that eventually produces #3111.

## Evidence

```rust
// crates/physics/src/water.rs:474-482 — apply_buoyancy, Stage::Physics
let surfaces = collect_water_surfaces(world);
let current_volumes = collect_water_current_volumes(world);   // <- WaterCurrentVolume
let time_secs = world.try_resource::<TotalTime>().map(|time| time.0);   // <- TotalTime
let atmospheric_wind = world
    .try_resource::<WindField>()                              // <- WindField
    .map(|wind| *wind)
    .unwrap_or_default();
```

```rust
// crates/core/src/ecs/components/water.rs:515-517 — it is a real storage, not a resource
impl Component for WaterCurrentVolume {
    type Storage = SparseSetStorage<Self>;
}
```

The `Access` chain at `boot.rs:1233-1290` declares `PhysicsWorld` (r/w), `PhysicsWaterConstants`,
`ContactConfig`, `FormIdPool`, and the components `CollisionShape`, `RigidBodyData`, `GlobalTransform`,
`RapierHandles` (r/w), `Transform` (w), `WaterPlane`, `WaterVolume`, `WaterFlow`, `WaterContact` (w),
`RenderLayer`, `FormIdComponent`, `PhysicsSourceForm`. None of the three above appear.

```rust
// byroredux/src/anim_convert.rs:29 — build_subtree_name_map, reached from
// byroredux/src/systems/animation.rs:65 on the subtree-cache miss path
let children_q = world.query::<Children>();
```

## Impact

No live race for either system. `sys.accesses` under-reports `physics_sync_system`'s read surface by
three entries and `make_animation_system`'s by one, and the analyzer will mis-clear any future
`Stage::Physics` sibling that writes wind, the engine clock, or placed current volumes — and any future
`Stage::Update` sibling that writes `Children`. Contract/observability defect with a real future-race
enabling property, which is exactly what #1787 was filed and fixed for.

## Related

- #1787 / CONC-D4-01 (`ContactConfig` on this very function — same family, fixed, its comment is the
  argument for this fix)
- #3111 / `ECS-2026-08-20-01` / `CONC-2026-08-20-01` (same family, **live parallel writer**, HIGH)
- `CONC-2026-08-20-03` (same family, exclusive system, LOW) — filed separately
- #2389, #2676 (same family, both CLOSED)

## Suggested Fix

Add to the `physics_sync_system` chain, alongside the existing WATAL entries:

```rust
.reads_resource::<byroredux_core::ecs::resources::TotalTime>()
.reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
.reads::<byroredux_core::ecs::components::water::WaterCurrentVolume>()
```

and to `make_animation_system()`:

```rust
.reads::<byroredux_core::ecs::Children>()
```

Unlike #3111 these additions **cannot** trip `known_conflict_count()` — all four are read-only and
neither stage has a second system to pair against.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every `add_to_with_access` /
      `add_exclusive_with_access` chain whose system body reaches a resource or storage through a
      cross-crate or cross-module call frame (`weather_wave_adjustment`, `build_subtree_name_map` are the
      two known shapes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix — the declared access set for these two systems
      should be asserted against the reads their bodies actually perform
