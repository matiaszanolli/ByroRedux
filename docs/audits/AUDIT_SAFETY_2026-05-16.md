# Safety Audit — 2026-05-16

## Scope

Full audit across the standard ten dimensions per `/audit-safety`: unsafe
Rust, Vulkan spec compliance, GPU/CPU memory, threading, cxx FFI, RT
pipeline, new compute pipelines (TAA / Caustic / Skin), R1 material
table, RT IOR-refraction, and NPC / animation spawn.

Contextual trigger: skinned-BLAS flag split landed in `1775a7e6`
(R6a-prospector-regress) — `UPDATABLE_AS_FLAGS` was split into a
per-acceleration-type pair. TLAS keeps the old constant
(`PREFER_FAST_TRACE | ALLOW_UPDATE`); skinned BLAS gets the new
`SKINNED_BLAS_FLAGS` (`PREFER_FAST_BUILD | ALLOW_UPDATE`). Five
call sites in `blas_skinned.rs` flip to the new constant. Audit
verifies no unsafe-block invariant breakage at the AS-build sites
and that the per-type flag constants do not expose new soundness
or UB hazards.

## Dedup pass

`gh issue list --state all --limit 200 --search …` against
keywords drawn from the touched code path
(`SKINNED_BLAS_FLAGS`, `UPDATABLE_AS_FLAGS`,
`PREFER_FAST_BUILD`, `R6a-prospector`, `REN-D8-NEW-08`,
`skinned BLAS flags`). Zero open issues match the new constant;
the related closed issues are `#958` (REN-D8-NEW-14 — original
shared-constant introduction, supersession is intentional) and
`#679` (AS-8-9 — refit-threshold rebuild policy, still in force).
Prior safety audit (`AUDIT_SAFETY_2026-05-11.md`) SAFE-25 / SAFE-26
both verified CLOSED (#950 / #951).

Existing open D2 / D3 / D6 / D10 issues from the 2026-05-11 audit
re-affirmed under their original numbers (#908, #909, #910, #913,
#947, #948, #949, #850, #856, #858, #661, #911, #946) — no new
state. Concurrency audit running in parallel already filed
`CONC-D2-NEW-01` (queue MutexGuard dropping before `queue_submit`);
referenced where the surface overlaps below.

## NEW findings

### SAFE-D1-NEW-01: Stale doc comments on three skinned-BLAS call sites still reference `UPDATABLE_AS_FLAGS` + `PREFER_FAST_TRACE`
- **Severity**: LOW
- **Dimension**: D1 (unsafe Rust — comment hygiene around three `unsafe { cmd_build_acceleration_structures }` call sites)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:92-98`, `:288`, `:650-652`
- **Status**: NEW (sibling-of `1775a7e6`)

**Description.** The 2026-05-16 `1775a7e6` fix flipped five skinned
BLAS BUILD/UPDATE call sites from `UPDATABLE_AS_FLAGS`
(PREFER_FAST_TRACE) to `SKINNED_BLAS_FLAGS` (PREFER_FAST_BUILD). The
code paths are correct (verified — see RE-AFFIRMED D6 below), but
three of the surrounding doc comments still describe the old
constant and the old rationale:

`blas_skinned.rs:92-98` — `build_skinned_blas` initial BUILD:
```rust
// Build flags: see `UPDATABLE_AS_FLAGS` for the shared
// PREFER_FAST_TRACE | ALLOW_UPDATE rationale (#679 / REN-D8-NEW-08:
// skinned BLAS refits in-place ~600 frames between full builds, so
// trace cost dominates by ~6 orders of magnitude). #958 lifted the
// four UPDATE-target call sites to the shared constant to enforce
// VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 by
// construction.
```
… followed by `.flags(SKINNED_BLAS_FLAGS)` on line 101. The comment
asserts the exact behaviour the bisect refuted: that PREFER_FAST_TRACE
is the right choice because refits dominate.

`blas_skinned.rs:288` — `build_skinned_blas_batched_on_cmd` setup:
```rust
// Flags: shared `UPDATABLE_AS_FLAGS` — see #958 / REN-D8-NEW-14.
```
… followed by `.flags(SKINNED_BLAS_FLAGS)` on lines 330 + 451.

`blas_skinned.rs:650-652` — `refit_skinned_blas` UPDATE call:
```rust
// mode = UPDATE: …  The shared
// `UPDATABLE_AS_FLAGS` constant guarantees this UPDATE's flag
// set matches the original BUILD (VUID-…-pInfos-03667). See
// #958 / REN-D8-NEW-14.
```
… followed by `.flags(SKINNED_BLAS_FLAGS)` on line 655.

**Impact.** Not a safety bug — the code is correct. But the comments
mislead any future reader who greps `UPDATABLE_AS_FLAGS` looking to
understand why the skinned-BLAS BUILD/UPDATE pair must stay
lockstep, and who then reverts to `UPDATABLE_AS_FLAGS` "to match the
documented design," reintroducing the 18% Prospector regression.
The hazard is doc-rot leading to flag drift on the next refactor;
the VUID-03667 BUILD/UPDATE flag-match invariant is still enforced
by every call site referencing the same constant, but the doc and
the constant disagree about which constant that is.

**Suggested Fix.** Replace `UPDATABLE_AS_FLAGS` with
`SKINNED_BLAS_FLAGS` in all three comment sites, and update the
rationale-paragraph at `:92-98` to reference R6a-prospector-regress
+ the empirical PREFER_FAST_BUILD outcome instead of the now-
falsified PREFER_FAST_TRACE prediction. The `constants.rs:71-77`
docblock already carries the correct history — propagate the same
explanation to the call-site comments.

---

### SAFE-D1-NEW-02: `SKINNED_BLAS_FLAGS` / `UPDATABLE_AS_FLAGS` not pinned by a unit test
- **Severity**: LOW
- **Dimension**: D1 / D6 (RT pipeline — invariant pin coverage)
- **Location**: `crates/renderer/src/vulkan/acceleration/constants.rs:78-98`, `tests.rs` (absent)
- **Status**: NEW (sibling-of `1775a7e6`)

**Description.** Both `UPDATABLE_AS_FLAGS` and `SKINNED_BLAS_FLAGS`
are `pub(super) const` bit-set composites built via
`vk::BuildAccelerationStructureFlagsKHR::from_raw(…)`. Neither
constant has a unit test pinning its bit set:

- `UPDATABLE_AS_FLAGS` should be `PREFER_FAST_TRACE | ALLOW_UPDATE`
  (no `ALLOW_COMPACTION`, no `PREFER_FAST_BUILD`).
- `SKINNED_BLAS_FLAGS` should be `PREFER_FAST_BUILD | ALLOW_UPDATE`
  (no `ALLOW_COMPACTION`, no `PREFER_FAST_TRACE`).

`tests.rs` covers `validate_refit_counts` (the post-BUILD count
invariant for VUID-03667) but nothing pins the BUILD flag set
itself. A typo on a future edit (`PREFER_FAST_BUILD` → `PREFER_FAST_TRACE`,
or accidentally adding `ALLOW_COMPACTION` to the skinned arm, which
would interact badly with `mode=UPDATE` per Vulkan spec)
compiles and runs — and the failure mode is silent perf regression
(or, in the ALLOW_COMPACTION case, a VUID violation at the next
UPDATE call: `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667`
requires the UPDATE's flags to match the original BUILD's flags
exactly, but ALLOW_COMPACTION-flagged BLAS cannot be refit
in-place — the validation layer would catch this in debug, but
release builds would silently mis-render the skinned mesh).

The pattern the rest of the renderer uses for this class of invariant
is a `gpu_material_size_is_260_bytes`-style `#[test]` (see
`material.rs:647`). Adding two analogous tests
(`updatable_as_flags_is_fast_trace_plus_allow_update`,
`skinned_blas_flags_is_fast_build_plus_allow_update`) makes the
flag-set drift the same class of CI failure as the GpuMaterial
layout drift.

**Suggested Fix.** Add to `tests.rs`:
```rust
#[test]
fn updatable_as_flags_is_fast_trace_plus_allow_update() {
    use ash::vk::BuildAccelerationStructureFlagsKHR as F;
    assert_eq!(
        super::constants::UPDATABLE_AS_FLAGS,
        F::PREFER_FAST_TRACE | F::ALLOW_UPDATE
    );
}
#[test]
fn skinned_blas_flags_is_fast_build_plus_allow_update() {
    use ash::vk::BuildAccelerationStructureFlagsKHR as F;
    assert_eq!(
        super::constants::SKINNED_BLAS_FLAGS,
        F::PREFER_FAST_BUILD | F::ALLOW_UPDATE
    );
}
```
The constants are `pub(super)` so the test must live inside the
`acceleration` module tree (which `tests.rs` already does).

---

### SAFE-D6-NEW-01: `BlasEntry::built_*_count` pin captures geometry counts but not the BUILD-time flag set
- **Severity**: LOW
- **Dimension**: D6 (RT pipeline — VUID-03667 invariant coverage)
- **Location**: `crates/renderer/src/vulkan/acceleration/types.rs:42`, `blas_skinned.rs:579-589` (refit validation site)
- **Status**: NEW (gap exposed by `1775a7e6`)

**Description.** `BlasEntry` (in `acceleration/types.rs`) pins
`built_vertex_count` and `built_index_count` at BUILD time so
`validate_refit_counts` can defend VUID-03667 against vertex/index-
count drift between BUILD and UPDATE. The flag-set portion of the
same VUID is enforced by source-code convention — every BUILD/UPDATE
pair references the same `*_AS_FLAGS` constant. That convention
held trivially when there was one shared constant; with two
constants on a similarly-named pair, a future BUILD site that
references `UPDATABLE_AS_FLAGS` while the corresponding UPDATE site
(`refit_skinned_blas`) references `SKINNED_BLAS_FLAGS` would
compile, run, and silently violate VUID-03667 (the validation layer
catches it in debug; release builds may garbage-render or device-
lost depending on driver).

The defense-in-depth fix mirrors what `built_vertex_count` /
`built_index_count` already do for counts: pin
`built_flags: vk::BuildAccelerationStructureFlagsKHR` into
`BlasEntry` at BUILD and assert at UPDATE that `entry.built_flags ==
flags_used_for_this_refit`.

**Impact.** The current code is correct — all five skinned BLAS
sites use the same constant. The hazard is purely future-proofing:
nothing structural prevents the constants from drifting at a
single call site. The 03667 BUILD/UPDATE-match invariant is the
load-bearing reason `1775a7e6` split the constant in the first
place (the commit message + the constant-level docblock both
explicitly call this out); the geometry-count pin already exists
for the count half of the same VUID, so the flag-set half is the
natural completion.

**Suggested Fix.** Add `built_flags: vk::BuildAccelerationStructureFlagsKHR`
to `BlasEntry`, populate at BUILD time in all 5 BUILD sites + the
2 TLAS BUILD sites, and assert match at UPDATE time inside the
existing `validate_refit_counts` (rename / extend the predicate
to `validate_refit_inputs`). Single point of failure; covered by
the existing tests.rs harness once the new constants get their
pin tests (SAFE-D1-NEW-02 above).

## RE-AFFIRMED invariants

### D1 — Unsafe Rust blocks (528 occurrences, +12 since 2026-05-11)

`grep -rn "unsafe " crates/` counted 528 unsafe occurrences across
the workspace (was 516 on 2026-05-11). Net growth +12 — sampled and
identified as new ash bindings in: skin_compute / acceleration
batched paths (unsafe by API design — `cmd_build_acceleration_structures`,
`get_buffer_device_address`, `create_acceleration_structure` —
bulk-trusted at the ash layer), `texture.rs` mip-upload, and
`debug-server` screenshot readback.

Non-trivial unsafe operations (raw-pointer / `from_raw_parts` /
slice casts) re-verified — all still carry explicit `SAFETY:`
comments with sound invariants. Spot-checked the SAFETY block at
`blas_skinned.rs:316-322` (PreparedSkinned `'static` geometry
union) — invariant unchanged by `1775a7e6` (the union still holds
value-typed fields only; only the `flags` field of the
`AccelerationStructureBuildGeometryInfoKHR` consumer changed
constant).

The five `unsafe { cmd_build_acceleration_structures }` call sites
in `blas_skinned.rs` (lines 101, 165, 330, 451, 655 — counted by
flag-constant grep) all dispatch to ash bindings that accept the
flag set without further validation. `1775a7e6` is a flag-value
change; the unsafe contract of the underlying call (cmd buffer in
recording state + scratch buffer alive + accel buffer alive) is
unchanged. Confirmed.

`World::get()` raw-pointer extension still absent (replaced by
`ComponentRef` at `crates/core/src/ecs/query.rs:188-235` under #35
— sound by construction).

### D2 — Vulkan spec compliance

Five-site skinned BLAS BUILD/UPDATE flag-match invariant
(VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667) re-verified
post-`1775a7e6`:

- 4 BUILD sites (`blas_skinned.rs:101, 165, 330, 451`) →
  `SKINNED_BLAS_FLAGS`
- 1 UPDATE site (`blas_skinned.rs:655` inside `refit_skinned_blas`)
  → `SKINNED_BLAS_FLAGS`

Pair is lockstep. TLAS pair (`tlas.rs:386` BUILD + `:722` UPDATE)
both use `UPDATABLE_AS_FLAGS` — also lockstep. Static BLAS
(`blas_static.rs:217, 300`) uses function-local `STATIC_BLAS_FLAGS`
(PREFER_FAST_TRACE | ALLOW_COMPACTION) — no UPDATE path so VUID
03667 not applicable; the compaction flag does not interact with
either updatable constant.

No new use-after-destroy paths introduced. `1775a7e6` is a constant
swap at the `BuildAccelerationStructureFlagsKHR` argument; touches
no resource lifecycle, no Drop chain, no swapchain recreation.

Existing open D2 issues (#908 / #909 / #910 / #913 / #947 / #948 /
#949) re-affirmed; concurrency audit's `CONC-D2-NEW-01` (queue
MutexGuard dropping before `queue_submit`) **does not overlap this
audit's safety surface** — the skinned BLAS flag-split call sites
are all command-buffer-record sites (not queue submit), and the
underlying `submit_one_time` helper (`predicates.rs`) was not
touched by `1775a7e6`. Cross-referenced as not-our-finding.

### D3 — Memory safety

GPU side. `1775a7e6` does not change any buffer creation, allocation,
or destroy path. The skinned BLAS scratch buffer
(`blas_scratch_buffer`) is still grow-only with VUID-aligned
device addresses (`debug_assert_scratch_aligned` calls at
`blas_skinned.rs:162, 436, 641` — three of which the audit prompt
flags directly). The PREFER_FAST_BUILD vs PREFER_FAST_TRACE choice
affects internal BVH layout, not buffer-size requirements; the size
query at `:106-113, :334-342` correctly re-queries
`get_acceleration_structure_build_sizes` against the new flag set,
so result-buffer and scratch-buffer allocations remain right-sized.

CPU side. No new HashMap / Vec growth paths. The two open
unbounded-growth findings from 2026-05-11 (#850 SoundCache, plus
the closed #951 BgsmProvider cache) are unrelated to the flag
split.

### D4 — Thread safety

`TypeId`-sorted lock acquisition unchanged. `1775a7e6` does not
add any new RwLock / Mutex sites. The five skinned BLAS call sites
each hold `self: &mut AccelerationManager` for their duration; the
manager is single-threaded by construction (owned by VulkanContext,
mutated from `draw_frame` only).

### D5 — FFI safety

`crates/cxx-bridge/src/lib.rs` still a placeholder; not touched
by `1775a7e6`. N/A.

### D6 — RT pipeline safety

Re-verified per the audit prompt's RT-specific list. Pertinent
changes:

- **BLAS / TLAS device-address sites**: unchanged. Buffer
  usage masks (`SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_*`)
  not touched by `1775a7e6`.
- **VUID-03667 BUILD/UPDATE flag-match**: the load-bearing
  invariant `1775a7e6` was written to preserve. Pair preserved on
  both the TLAS arm and the skinned BLAS arm. See SAFE-D6-NEW-01
  for the defense-in-depth follow-up (pin BUILD flags into
  `BlasEntry`).
- **`validate_refit_counts` (#907 / REN-D12-NEW-01)**: at
  `predicates.rs:85`. Still in place. Defends the count half of
  VUID-03667; covered by 4 unit tests at `tests.rs:156-188`. Flag
  half is the gap SAFE-D6-NEW-01 names.
- **Skinned BLAS refit threshold**: `SKINNED_BLAS_REFIT_THRESHOLD`
  = 600 unchanged at `constants.rs:54`. Drop-and-rebuild after 600
  refits still in force (see `blas_skinned.rs:698-703`
  `should_rebuild_skinned_blas`). The PREFER_FAST_BUILD constant
  swap does not interact with the threshold — both
  PREFER_FAST_TRACE and PREFER_FAST_BUILD produce equally
  ALLOW_UPDATE-compatible BVHs.
- **Skin compute output buffer usage**: at
  `skin_compute.rs:327-329` — `STORAGE_BUFFER |
  SHADER_DEVICE_ADDRESS |
  ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. The audit
  prompt's "M29.3 also re-adds VERTEX_BUFFER" note still applies:
  Phase 3 (raster reading skinned output as VBO) is not yet
  landed. Current mask is correct for the Phase 2 RT-only
  consumer. Will need to re-add VERTEX_BUFFER when M29.3 lands.

Open D6 issues (#661 legacy AS-READ flag, #911 first-sight prime
stall) re-affirmed; not affected by `1775a7e6`.

### D7 — New compute pipeline safety (TAA, Caustic, Skin)

Unchanged surface. TAA / Caustic / Skin compute pipelines all
re-verified clean per 2026-05-11. SPIR-V reflection
(`validate_set_layout`) call-site enumeration unchanged.
SAFE-25 (main raster pipeline reflect-validation gap) was closed at
#950; the related pipeline.rs build path is now covered.

### D8 — R1 material table

Unchanged. `GpuMaterial` 260 B size pin
(`gpu_material_size_is_260_bytes` at `material.rs:647`) and
per-field offset pin
(`gpu_material_field_offsets_match_shader_contract` at
`material.rs:675`) both in place. SAFE-22 over-cap return-0
(#797) and SAFE-25 ui.vert lockstep (#785) re-verified at the
existing line ranges.

### D9 — IOR refraction safety

Unchanged. Frisvad basis (`triangle.frag:316-322`), texture-equality
identity check (`:1684`), GLASS_RAY_BUDGET = 8192 (`:1534`),
interior cell-ambient fallback (`:1775`) all in place.
`DBG_VIZ_GLASS_PASSTHRU = 0x80` debug bit catalog
(`:659-741`) unchanged.

### D10 — NPC / animation spawn

Unchanged. B-spline FLT_MAX sentinel
(`anim.rs:2017-2020`), AnimationClipRegistry
case-insensitive dedup
(`registry.rs:46-117`), all in place. `1775a7e6` is renderer-only;
animation surface unaffected.

## Report Finalization

Three NEW findings, all LOW severity (defense-in-depth + doc-rot
sibling of `1775a7e6`). 13 existing OPEN issues re-affirmed under
their original numbers; no regressions detected against the
2026-05-11 baseline. The skinned-BLAS flag split itself is
correct — VUID-03667 BUILD/UPDATE flag-match invariant preserved
across all five skinned BLAS call sites; no new unsafe-block
hazards; the per-acceleration-type constant pair does not expose
any new soundness or UB surface beyond the doc-rot risk captured
in SAFE-D1-NEW-01.

CONC-D2-NEW-01 (queue MutexGuard / `queue_submit` lifetime — filed
by parallel concurrency audit) does NOT overlap this audit's
surface: the touched skinned BLAS sites are command-buffer-record
sites, not queue-submit sites; `submit_one_time` (the only
queue-submit helper on the path) was unchanged by `1775a7e6`.

| Finding              | Severity | Action                                                                                                |
|----------------------|----------|-------------------------------------------------------------------------------------------------------|
| SAFE-D1-NEW-01       | LOW      | Fix three stale comments in `blas_skinned.rs` (point at `SKINNED_BLAS_FLAGS`, drop FAST_TRACE rationale) |
| SAFE-D1-NEW-02       | LOW      | Add two `#[test]` pins for `UPDATABLE_AS_FLAGS` + `SKINNED_BLAS_FLAGS` bit composition                  |
| SAFE-D6-NEW-01       | LOW      | Pin `built_flags` into `BlasEntry`; extend `validate_refit_counts` to enforce BUILD/UPDATE flag-match  |

All three findings are sibling-of `1775a7e6` and can be addressed
in a single follow-up commit. None block any current functionality
— `1775a7e6` is correct as landed; these are defense-in-depth
follow-ups for the next refactor of the AS flag domain.

Suggested next:

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-05-16.md
```
