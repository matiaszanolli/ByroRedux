# 4145: SAVE-D6-2026-09-11-01: extensions preflight/restore never named in save-load-roundtrip.md's numbered ordering

State: OPEN  Labels: ['documentation', 'low', 'save-load', 'doc-rot']

### SAVE-D6-2026-09-11-01: extensions preflight/restore are never named in `save-load-roundtrip.md` §6's numbered ordering list, only in a disconnected prose section

- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply (documentation)
- **Data-Loss Class**: none (doc rot / doc gap)
- **Location**: `docs/engine/save-load-roundtrip.md:138-236` (§6's 8-step numbered list) vs. `:261-292` ("Engine-native extension state")
- **Status**: NEW. `24df5304` (sandboxed extensions) landed after the 2026-08-30 audit; the doc section was added in the same era but never folded into §6's step list.
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: Neither `crate::extensions::preflight_extension_state` (real step 3, between the typed preflight and the #3789 pre-reload `restore_resources`) nor `crate::extensions::restore_extension_state` (real step 7, between the reload and the post-reload `restore_resources`) appears in §6's numbered trace — confirmed by reading the full 8-step list during publish. The separate "Engine-native extension state" section covers the right invariants in prose but names neither function nor its position relative to §6's steps.

**Impact**: Documentation only — the substance is accurate in isolation, but a reader following §6 to understand control flow has no way to learn these two calls exist or where they sit. This is exactly the kind of gap that let a real ordering bug (`SAVE-D6-2026-08-30-01`) through three audit cycles unnoticed.

**Related**: `24df5304`; `SAVE-D6-2026-08-30-01`/`-04` (the closed findings this section is adjacent to).

**Suggested Fix**: Fold two bullets into §6's numbered list — "1b. Extensions preflight" between steps 1 and 2, "6b. Extensions restore" between the reload and "Restore whole resources" — each cross-referencing the existing prose section rather than duplicating it.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix


---

# 4146: AUD-2026-09-11-D6-01: reverb_zone_system ordering comment drifted stale a second time (regression of #3522)

State: OPEN  Labels: ['documentation', 'low', 'audio', 'doc-rot']

### AUD-2026-09-11-D6-01: `reverb_zone_system`'s ordering-guarantee comment has drifted stale for the second time — wrong file (again) and a mechanism that was never actually correct

- **Severity**: LOW
- **Dimension**: Reverb Send & Routing / Manager Lifecycle & ECS/Cell Streaming / Gameplay Audio Wiring (independently re-derived by three dimension agents in the source audit; reported once here)
- **Location**: `byroredux/src/systems/audio.rs:55-58`
- **Status**: Regression of #3522 (closed 2026-08-31 by `4dabfbaf`, which fixed the file-attribution half of the original finding but restated, rather than corrected, the mechanism half — and has since drifted stale on the path too, following the unrelated `8c5e02aa` `boot.rs` → `boot/` split on 2026-09-09). Confirmed still present at HEAD during publish.
- **Source**: `docs/audits/AUDIT_AUDIO_2026-09-11.md`

**Description**: The live comment on `reverb_zone_system` reads:
```rust
/// Runs in `Stage::Late` alongside `audio_system` — registered earlier
/// in `boot.rs::build_scheduler` (systems within a stage run in
/// registration order) so the send level is in place before any new
/// spatial track gets constructed this frame.
```
Two independent problems:

1. **Stale file reference.** `boot.rs` no longer exists — confirmed absent during publish (`ls byroredux/src/boot.rs` → not found); it was split into `byroredux/src/boot/` under commit `8c5e02aa` ("refactor(boot): split boot.rs into boot/ — one file per concern", 2026-09-09). `reverb_zone_system`'s actual registration is at `byroredux/src/boot/schedule/late.rs:165-171`, inside `register_late_systems`.
2. **The stated mechanism was never correct for this pairing, even before the file split.** "Systems within a stage run in registration order" is true for *exclusive-vs-exclusive* ordering (a plain sequential `Vec` iteration — `crates/core/src/ecs/scheduler.rs:511-514`) but false for the *parallel-vs-exclusive* relationship this comment is actually describing. `reverb_zone_system` is registered via `add_to_with_access` (`late.rs:165`), landing in `Stage::Late`'s **parallel** vector — confirmed directly during publish; `audio_system` is a bare `add_exclusive` (`late.rs:234`, confirmed), landing in the **exclusive** vector. `Scheduler::run` (`scheduler.rs:497-514`) always drains a stage's entire parallel batch (via a blocking `rayon::par_iter_mut`) before running a single entry from its exclusive list — this is what actually guarantees the ordering, and it is completely independent of which call textually appears first in `late.rs`. In fact `reverb_zone_system` is registered *after* three other `Stage::Late` **exclusives** (`make_billboard_system`, `footstep_system`, `ragdoll_writeback_system` — `late.rs:71-121`) and still runs before every one of them, precisely because it isn't one.

#3522's original filing already correctly identified mechanism (2) as the deeper problem, but the closing commit `4dabfbaf` only fixed clause (1) — it swapped `main.rs` for `boot.rs::build_scheduler` — while *restating* clause (2) in slightly different words rather than adopting the language #3522 itself already proposed. The mechanism claim was never actually corrected; it was reworded, and has now drifted stale on the path too as an unrelated refactor moved the registration site nine days later.

**Evidence**: `late.rs:165-171` (`add_to_with_access`, parallel) vs. `late.rs:234` (`add_exclusive`, exclusive) — both confirmed directly during publish; `scheduler.rs:93-99` (`StageData`'s two-vector layout) and `scheduler.rs:497-514` (`Scheduler::run`'s two-phase per-stage loop); contrast with the *correct* statement of this exact mechanism already in-tree, `late.rs:225-234` ("registered as **exclusive** so it sequences after the Late parallel batch... exclusive sequencing makes the dependency structural"), and in `byroredux/src/boot/schedule/mod.rs:27-34`'s own top-of-file explanation.

**Impact**: Documentation-only — the runtime behavior is correct and the ordering is structurally guaranteed, not incidental. The blast radius is a future refactor: this is exactly the comment a maintainer reads while touching `reverb_zone_system`, and it teaches a false mental model ("registration order" as a blanket rule) that a plausible future change — converting `reverb_zone_system` to an exclusive registered after `audio_system`, the same kind of parallel→exclusive conversion `make_billboard_system` and `footstep_system` already underwent (#3652) — would silently violate without any warning that the real invariant (staying in the `.parallel` bucket while `audio_system` stays `.exclusive`) had broken.

**Related**: #3522 (closed; this is the unfixed remainder of its own original two-part finding, now further stale on path); #3855/`8c5e02aa` (the boot split that broke the file reference); #3652 (the parallel→exclusive conversions that make the mechanism risk concrete rather than hypothetical).

**Suggested Fix**: Replace the parenthetical with the mechanism, not a file path that can drift again: state that `reverb_zone_system` is a `Stage::Late` **parallel** registration and `audio_system` a `Stage::Late` **exclusive**, and that `Scheduler::run` completes a stage's entire parallel batch before starting its exclusive list (`crates/core/src/ecs/scheduler.rs`) — independent of where either call appears in `boot/schedule/late.rs`. Consider a lightweight test that greps `late.rs` for both registrations and asserts `reverb_zone_system` uses `add_to*` while `audio_system` uses `add_exclusive*`, so a future accidental swap trips a test instead of relying on a comment surviving the next refactor.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — grep `late.rs` for both registration calls and assert the parallel/exclusive split, so a third recurrence of this drift is a test failure instead of a documentation nit


---

# 4172: D7-2026-09-11-01: parse_mgef's remapped associated_item sentinel (0xFFFFFFFF) triggers a false-positive warning on every multi-master load

State: OPEN  Labels: ['bug', 'medium', 'esm-plugin']

**Severity**: MEDIUM
**Dimension**: ESM→ECS Handoff
**Record / Sub-record**: `MGEF` / `DATA` (`associated_item` @8)
**Location**: `crates/plugin/src/esm/records/misc/magic.rs:681` (remap call), `:649` (sentinel doc); `crates/plugin/src/esm/reader.rs:449-491` (`FormIdRemap::remap`, out-of-range branch)
**Status**: NEW — residual of the #4070/D7-02 fix, which correctly remapped the field but did not add the sentinel guard its own suggested fix explicitly asked for.

**Description**: `#4070` correctly remapped `associated_item` and `effect_shader_id`. But `associated_item`'s documented "no item" sentinel is `0xFFFF_FFFF`, and `remap_fid` only special-cases `raw == 0` before delegating to `FormIdRemap::remap`. For `raw = 0xFFFF_FFFF`, `mod_index = 255` — on any real multi-master load this is neither a self-reference, an in-range master, nor (with masters present) the empty-master-list arm, so it falls to the final `else` arm, written for "genuinely suspicious" malformed input, which unconditionally `log::warn!`s. The value round-trips correctly (no corruption), but every MGEF record with no associated item — common, well-documented, authored data — now logs a warning misclassified as suspicious.

**Evidence**:
```rust
// misc/magic.rs:678-681
out.associated_item = remap_fid(header.associated_item, remap);
...
out.light_form_id = remap_fid(header.light_form_id, remap);
```
```rust
// reader.rs:483-490
} else {
    // Multi-plugin load with an out-of-range index — genuinely
    // suspicious (malformed file or an in-memory injected form).
    log::warn!("FormID {raw:08x} has mod_index {mod_index} but plugin has {} masters", ...);
    return raw;
}
```
No test exercises the real `0xFFFF_FFFF` sentinel under a non-identity remap (the existing test uses `associated_item = 0`, which short-circuits earlier).

**Impact**: Log noise, not data corruption — but a normal multi-master DLC load produces one `warn!` per no-item MGEF record at parse time, drowning out the rare genuine malformed-FormID case the branch exists to catch.

**Related**: #4070 (ESM-2026-09-09-D7-02) — this is a residual of that fix, which named this exact gap in its own suggested fix.

**Suggested Fix**: Guard the sentinel before calling `remap_fid` (`if header.associated_item == 0xFFFF_FFFF { 0xFFFF_FFFF } else { remap_fid(...) }`), or add a documented second early-out to `remap_fid` itself. Add a positive test with the real sentinel under a non-identity `FormIdRemap`.

## Completeness Checks
- [ ] **TESTS**: A positive test with `associated_item = 0xFFFF_FFFF` under a non-identity multi-master `FormIdRemap` pins the corrected no-warning behavior


---

# 4173: D7-2026-09-11-02: CELL's XEZN (encounter zone) sub-record is never parsed anywhere in the crate

State: OPEN  Labels: ['bug', 'medium', 'esm-plugin', 'doc-rot']

**Severity**: MEDIUM
**Dimension**: ESM→ECS Handoff
**Record / Sub-record**: `CELL` / `XEZN` (missing); cross-referenced type `ECZN`
**Location**: `crates/plugin/src/esm/cell/walkers.rs` (CELL sub-record match arms — every sibling `XCxx`/`XLxx` field present, `XEZN` absent from both, falls into the `_ => {}` catch-all); doc gap at `docs/engine/lighting-from-cells.md:513-516`; `EcznRecord` at `crates/plugin/src/esm/records/misc/world.rs:1183-1200`
**Status**: NEW

**Description**: `EcznRecord`'s own doc comment states encounter zones govern "spawn scaling / faction ownership on the cells that reference it via `XEZN`", and `ECZN` records are fully parsed, remapped and tested. But the CELL-side half — the `XEZN` sub-record that would populate a FormID field to look that map up by — has no match arm anywhere (`grep -rn 'b"XEZN"' crates/plugin/src/` returns nothing), unlike every other Skyrim-era `XCxx` CELL field (`XCIM`, `XCWT`, `XCAS`, `XCMO`, `XCCM`, `XLCN`, all present and remapped). A separate doc, `docs/engine/lighting-from-cells.md:513-516`, incorrectly claims this metadata lives on `CellData`/`CellOwnership` — neither struct has such a field.

**Evidence**: Confirmed via `grep -n "XEZN\|XCIM\|XCWT\|XCAS\|XCMO\|XCCM\|XLCN" crates/plugin/src/esm/cell/walkers.rs` — every sibling arm present, `XEZN` absent, falling to the catch-all.

**Impact**: Latent — no consumer reads `EsmIndex.encounter_zones` today, so nothing renders wrong now. But the sub-record is silently absorbed with no debug log, no TODO, and no "not yet parsed" doc mention, so a future encounter-zone/leveled-spawn feature will find the zone map populated but silently un-linkable to any cell, with a doc actively pointing at fields that don't exist.

**Related**: none filed; same "guard/doc says covered, code doesn't cover it" class as #4084/ESM-2026-09-11-D4-01's history.

**Suggested Fix**: Add `pub encounter_zone_form: Option<u32>` to `CellData`, add a `b"XEZN" => encounter_zone_form = read_form_id(reader, &sub.data)` arm next to the `XCCM`/`XLCN` arms in both CELL match blocks, and correct the `lighting-from-cells.md` doc reference.

## Completeness Checks
- [ ] **TESTS**: A fixture CELL record with an authored `XEZN` sub-record pins the new field


---

# 4183: CONC-D3-01: `submersion_system` samples `TotalTime`/`WindField` underneath the `WaterPlane`/`WaterVolume` storage guards, inverting the documented snapshot-before-storage discipline

State: OPEN  Labels: ['bug', 'medium', 'water', 'concurrency']

**Description**: `submersion_system` binds `world.query::<WaterPlane>()` and `world.query::<WaterVolume>()`, then acquires `try_resource::<TotalTime>()` and, inside the closure with that guard still live, calls `weather_wave_adjustment(world, time.0)`, which itself acquires `try_resource::<WindField>()`. Both water storage guards stay live until an explicit `drop` later in the function. This records four lock-order edges nothing else in the tree records: `WaterPlane/WaterVolume -> TotalTime/WindField`. The other three consumers of the identical wave-parameter pair all snapshot the frame-global resources into plain scalars *before* taking any water storage guard — one of them documents this explicitly as "resource-snapshot-before-storage discipline (#3265)". No live cycle exists today (checked every `WindField`/`WaterPlane`/`WaterVolume` acquisition site) — this is latent, not live.

**Evidence**:
`byroredux/src/systems/water.rs` (confirmed): `wq`/`vq` queries bound first, then `wave_adjustment` computed via `try_resource::<TotalTime>()` while those guards are still in scope, guards dropped later in the function. `crates/physics/src/water.rs:637-645` and `byroredux/src/render/water.rs:102-131` both follow the correct resource-first order; `character.rs:1043-1056` documents the discipline explicitly.

**Impact**: (a) The moment any code takes `TotalTime`/`WindField` before a water storage read — the natural spelling, since 3 of 4 existing sites do resource-first — the graph closes a real ABBA cycle and a `BYRO_LOCK_ORDER_CHECK=1` run aborts. (b) `submersion_system` is currently `add_exclusive_with_access`, so today's safety is circumstantial — promoting it to a parallel lane is a one-line change with no compile-time or test-time signal.

**Related**: #3265 (the discipline violated), #2388/#313 (inverted-pair debug aborts), `docs/engine/ecs.md` Sec Canonical acquisition order.

**Suggested Fix**: Hoist the frame-global sample above the storage guards to match `player_water_state` exactly — move the `wave_adjustment` binding before the `query::<WaterPlane>()` call, as a plain `Option` scalar. Three lines moved, no behavioral change.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*

---

# 4190: PERF-D1-2026-09-11-02: `scene_trigger_actor_approach_system` still deep-clones every `ScenePlayer`'s unused heap fields every frame

State: OPEN  Labels: ['bug', 'low', 'performance', 'scripting']

**Description**: Residual gap in a system already partially fixed by closed issue #3838 ("deep-clones every ScenePlayer into a fresh Vec each frame"). The outer-container reallocation that #3838 targeted is gone (closure-captured scratch now in place), but the per-entity deep clone of `active_actions: Vec<...>` / `completed_actions: HashSet<u32>` remains, and neither is read downstream — only three `Copy` scalar fields are.

**Evidence**:
`byroredux/src/systems/cinematic.rs:452-458` clones the full `ScenePlayer` including its two heap collections despite only three scalar fields being consumed by the three downstream consumer passes.

**Impact**: Per-entity heap allocation (Vec + HashSet clone) every frame for every actor with an active `ScenePlayer`, for data never read. Bounded by concurrently-active scene players, not by scene size.

**Related**: Closed #3838 (fixed the outer-container half of this same system).

**Suggested Fix**: Clone only the three scalar fields actually needed, or iterate the query directly for the three consumer passes instead of building an intermediate cloned collection.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*

---

# 4200: PERF-D4-2026-09-11-02: `memory-budget.md`'s scene-buffer total (225 MB) is exactly one `GpuInstance`-growth stale

State: OPEN  Labels: ['documentation', 'low', 'performance', 'doc-rot']

**Description**: The page's own row table was updated for #3231's `GpuInstance` 128->160 B growth, but the two totals below it (225 MB, ~223 MB) were not — they reproduce exactly the pre-#3231 sum. Current row sum is approx 242.3 MB (delta = 262144 x 32 B x 2 FIF = 16.8 MB, reproduced exactly).

**Evidence**:
`docs/engine/memory-budget.md:109,793`.

**Impact**: Under-counts resident scene VRAM by ~17 MB (7%) for any reader of the doc, including audit skill instructions that cite "~223-225 MB".

**Related**: #3231 (the growth this doc missed).

**Suggested Fix**: Update both totals to approx 242 MB; state the MiB/MB unit convention once in the table header (the table currently mixes both under one "MB" label).



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*

---

# 4202: PERF-D5-2026-09-11-01: `CLAUDE.md` still attributes ACES tone mapping to the composite pass

State: OPEN  Labels: ['documentation', 'low', 'performance', 'doc-rot']

**Description**: Composite now emits render-resolution linear HDR; exposure + ACES moved to `presentation.frag` downstream of the upscale boundary (confirmed: `composite.frag`/`composite.rs` contain no ACES/tonemap symbol; `presentation.frag:43,162` defines and applies `aces()`; `docs/engine/fsr3-upscaler-integration-plan.md:133` records the move). `CLAUDE.md`'s lines still describe the old shape.

**Evidence**:
`CLAUDE.md:146,159`.

**Impact**: Actively misleading for the frame's most safety-critical ordering question — bloom runs *after* composite on the (correctly) linear-HDR image; a reader trusting the stale `CLAUDE.md` line would file that ordering as a MEDIUM defect and be wrong (a plausible false positive for every future pass over this area).

**Related**: None named.

**Suggested Fix**: Reword both lines to describe composite as linear-HDR reassembly and attribute exposure/ACES to `presentation.vert/frag` (which currently has no `CLAUDE.md` entry at all).



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*

---

# 4206: PERF-D8-2026-09-11-01: `decode_bs_vertex_stream`'s tangent array grows by repeated push instead of the pre-sized allocation its sibling decoder already uses

State: OPEN  Labels: ['bug', 'nif-parser', 'medium', 'performance', 'nif']

**Description**: `decode_bs_vertex_stream` — the shared packed-vertex decoder for essentially all Skyrim SE+/FO4+/Starfield static and skinned geometry — pre-sizes `vertices`/`uvs`/`normals`/`vertex_colors` unconditionally and `bone_weights`/`bone_indices` conditionally on `is_skinned` (both correct). `tangents` is left `Vec::new()` regardless of `vertex_attrs`, even though whether it will be pushed on every iteration is exactly as knowable in advance (`VF_VERTEX && VF_TANGENTS && VF_NORMALS`, fixed for the whole call) as the skinning case. The sibling SSE-reconstruction decoder for the same on-disk format (`sse_recon.rs`, #559) already applies this exact conditional pre-size for `tangents` — the fix is known and landed elsewhere but wasn't carried to the more heavily-hit inline `BsTriShape::parse` path.

**Evidence**:
`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1138` (`let mut tangents: Vec<[f32; 4]> = Vec::new();`, confirmed never resized) vs `sse_recon.rs:341-345` (`if has_tangent_quad { Vec::with_capacity(num_vertices) } else { Vec::new() }`). Existing dhat coverage (`heap_allocation_bounds.rs`'s `bs_tri_shape_block_with_vertices` fixture) explicitly omits `VF_TANGENTS` per its own comment, so this path has never been under a heap-allocation bound.

**Impact**: For every normal-mapped Skyrim SE+/FO4+/Starfield mesh — the dominant case, since `VF_TANGENTS` is set pervasively wherever a mesh authors a normal map — `tangents` grows via default doubling-capacity reallocation instead of one reservation: `log2(num_vertices)` extra realloc+copy cycles per mesh block. Bounded (amortized O(1), not O(n^2)) but exactly the regression class #3691 fixed elsewhere in the same file family.

**Related**: #3691 (skin buffer pre-sizing, sibling fix); #2114/D8-02 (the dhat fixture with the coverage gap this finding identifies).

**Suggested Fix**: Hoist the `has_tangent_quad` check before the loop and pre-size via `stream.allocate_vec(nv_u32)?`, mirroring the `is_skinned` gate and `sse_recon.rs` verbatim. Extend the dhat fixture with `VF_TANGENTS` set so a future revert trips the existing gate.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*

---

# 4207: PERF-D8-2026-09-11-02: `pre_parse_cell`'s per-REFR model-path loop allocates two-to-three throwaway strings before the dedup check that would make most of them unnecessary

State: OPEN  Labels: ['bug', 'nif-parser', 'medium', 'performance']

**Description**: For every REFR in a cell (not just unique models), the loop does a full-string `.to_ascii_lowercase()` purely to test `.ends_with(".spt")` (discarded), then calls `canonical_model_path_key` — itself a second `to_ascii_lowercase().replace(...)` plus, conditionally, a `format!` allocation — before the cache/batch dedup check that determines whether the computed key is even used. The function's own comments document ~95% cache-hit rate on a 7x7 exterior grid and heavy per-cell model-path duplication (chairs/lanterns/rocks sharing one path each) — meaning these 2-3 allocations per REFR run essentially every time regardless of outcome.

**Evidence**:
`byroredux/src/streaming.rs:1430-1472` (allocations at :1452,:1460), `byroredux/src/cell_loader/nif_import_registry.rs:49-56` (`canonical_model_path_key` at :50,:54).

**Impact**: Bounded, load-path (streaming-worker thread, not render-frame) allocation churn proportional to `cell.references.len()` rather than the unique-model count, which the surrounding comments document as roughly an order of magnitude smaller. Cell-load latency waste, not a frame-time regression.

**Related**: Distinct from #3038's correctness fix and #877/#830/#1262's phase-split work, unaffected by this finding.

**Suggested Fix**: Non-allocating suffix test for `.spt`; memoize `canonical_model_path_key` per-call keyed on the raw `&str`, or check the dedup sets with a borrowed lowercase compare before committing to an owned `String`.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*

---
