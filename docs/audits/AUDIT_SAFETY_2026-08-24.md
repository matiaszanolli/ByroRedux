# Safety Audit — 2026-08-24

Scope: all 11 dimensions, full comprehensive run (no `--focus` filter),
standalone `/audit-safety` invocation. This audit sits one day after
`docs/audits/AUDIT_SAFETY_2026-08-23.md` (part of `nif-deep`), which did a
complete dimension-by-dimension sweep and left two HIGH findings open as
issues #3237/#3238. Rather than re-deriving all eleven dimensions from
scratch, this run (a) re-verified every regression guard against current
HEAD, (b) specifically diffed and read every file changed since that
report (`5db3b0b9..HEAD`, ~30 commits: quest-fragment dispatch, save-load
notifications, GPU morph-target blending Phase D, NAVM pathfinding, and
the squashed fix commit `06f86742`), and (c) ran the full workspace test
suite crate-by-crate as the sound evidence channel this domain leans on.

## Executive Summary

**1 NEW HIGH finding, 0 NEW MEDIUM, 1 recurring LOW, 0 CRITICAL.** Both
HIGH findings carried over from 2026-08-23 (#3237 GRUP-recursion depth
bound, #3238 Ball/Capsule/Cylinder shape-extent clamp) are **now FIXED**
— both landed in the same squashed commit (`06f86742`, today) that also
added a debug-UI toast and save-format validation. Two of the three LOW
doc-drift findings from 2026-08-23 are also now fixed; one recurs.

The one NEW finding is not a memory-safety or Vulkan-spec defect — it is
a **build regression that currently breaks `cargo test` and
`cargo test --workspace`**, the project's primary verification commands
per `CLAUDE.md`'s Quick Reference. A same-day `Effect` enum growth
(`Conditional`/`SetGlobalValue`/`Disable` variants, quest-fragment work)
was not propagated to an exhaustive `match` in a scripting example,
so the whole workspace test build currently fails before any test runs.
The underlying logic is *not* broken — `cargo test -p byroredux-scripting
--lib` (311 tests) passes clean — but the default, documented command
does not, which is itself the safety-relevant fact: nothing running the
documented workflow can currently confirm any of this report's PASS
verdicts by rerunning the suite.

Every crate in the workspace was independently test-run this session
(`byroredux-core`, `-physics`, `-plugin`, `-renderer`, `-mod-runtime`,
`-save`, `-scripting` (`--lib`), `-ui`, `-nif`, `-audio`, `-facegen`,
`-hkx`, `-pex`, `-bsa`, `-debug-server`, `-debug-protocol`, `-spt`,
`-bgsm`, `-sfmaterial`, `-fsr3-sys`, `-cxx-bridge`, `-papyrus`, and the
`byroredux` binary itself — 1515 passing). All passed except the one
example build break noted above. `cargo check --workspace` is clean.

## Findings

### SAFE-BUILD-2026-08-24-01 — `cargo test --workspace` fails to build: `fragment_coverage.rs` example doesn't cover three new `Effect` variants (HIGH)

- **Dimension**: Cross-cutting (build health / test-verification gate) —
  not one of the eleven numbered dimensions, but reported here because it
  currently blocks the sound-evidence channel (`cargo test`) every other
  dimension in this audit domain relies on.
- **Location**: `crates/scripting/examples/fragment_coverage.rs:59` (the
  non-exhaustive `match e { … }`); the enum it matches is
  `crates/scripting/src/translate/effects.rs:68` (`pub enum Effect`).
- **Status**: NEW. No existing open issue references `fragment_coverage`,
  `Effect::Conditional`, or a non-exhaustive-match build break (checked
  against the 200 most recent issues, `/tmp/audit/issues.json`).
- **Description**: Commits `cee35507` ("Implement global variable
  management and conditional effects in quest fragments") and `5f38402e`
  ("Add ReferenceEnableState … implement Disable effect") added three new
  `Effect` variants — `Conditional { .. }`, `SetGlobalValue { .. }`, and
  `Disable { .. }` — today. `fragment_coverage.rs`'s exhaustive `match e`
  (last touched `20d74b05`, before those three variants existed) was not
  updated, so it now fails to compile with `E0004` (non-exhaustive
  patterns). Verified reproducible right now:
  ```
  $ cargo test --workspace --quiet
  error[E0004]: non-exhaustive patterns: `&Effect::Conditional { .. }`,
    `&Effect::SetGlobalValue { .. }` and `&Effect::Disable { .. }` not covered
    --> crates/scripting/examples/fragment_coverage.rs:59:11
  error: could not compile `byroredux-scripting` (example "fragment_coverage")
    due to 1 previous error
  ```
  `cargo test --workspace` builds every workspace target — including
  examples — before running any test binary; a build failure in one
  target aborts the whole invocation with **zero tests executed**, for
  any crate, anywhere in the workspace. `cargo test -p byroredux-scripting`
  (no `--lib` flag, exactly as `CLAUDE.md`'s Quick Reference documents for
  other crates) fails identically.
- **Impact**: The project's two documented top-level verification
  commands (`cargo test`, `cargo test -p <crate>`) are currently broken
  workspace-wide. This is not a logic regression — isolating with
  `cargo test -p byroredux-scripting --lib` shows all 311 lib tests
  (including every new quest-fragment/trigger/quest-stage test added this
  session) pass cleanly — but anyone or any CI step invoking the
  documented command sees only a compile error, not the real pass/fail
  state of the suite. That is itself a safety-relevant condition: a
  future *actual* regression introduced alongside or after this break
  could go unnoticed if the visible symptom ("`cargo test` errors") is
  already assumed-explained by this known issue.
- **Suggested Fix**: Add the three missing arms to `fragment_coverage.rs`'s
  `match` (label them, matching the existing per-variant style — the
  compiler's own suggestion block gives the exact arm signatures), or
  fold them under a documented wildcard if the example's coverage-report
  intent doesn't need per-variant detail. One-line-per-variant fix;
  low risk.

## Dedup / Carry-Forward — 2026-08-23 Findings Re-Verified Against Current HEAD

### SAFE-D2-2026-08-23-01 (Existing: #3237) — RESOLVED

GRUP-tree recursion now threads a `depth: u32` through
`extract_records`/`extract_records_with_modl`/`extract_dial_with_info`/
`extract_quest_dialogue_scene_tree_inner`
(`crates/plugin/src/esm/records/grup_walker.rs`) and calls
`reader.bounded_group_content_end(&sub_group, depth, …)`, which is bounded
by the new `MAX_GRUP_NESTING_DEPTH: u32 = 64` constant
(`crates/plugin/src/esm/reader.rs:32`) — the exact fix shape the
2026-08-23 report suggested, mirroring `MAX_NIF_NODE_DEPTH`. A dedicated
regression test (`crates/plugin/src/esm/reader.rs` test module, `for _ in
0..(MAX_GRUP_NESTING_DEPTH + 128)`) asserts an over-depth GRUP is skipped
without aborting the parse and outer byte accounting is preserved. Landed
in `06f86742` (today, same-day as this audit). `byroredux-plugin`'s full
802-test suite (779 passed, 23 opt-in-ignored) is green. Issue #3237 is
still open on GitHub (`closedAt: null`) — the fix has not yet been used to
close it; recommend closing on next triage pass.

### SAFE-D9-2026-08-23-01 (Existing: #3238) — RESOLVED

`Ball`, `Capsule`, and `Cylinder` arms of `flatten_to_parts`
(`crates/physics/src/convert.rs`) now all route through
`clamp_shape_extent()` (`value.clamp(1e-3, MAX_SANE_SHAPE_EXTENT)`,
`MAX_SANE_SHAPE_EXTENT = 1_048_576.0`) exactly like the `Cuboid` arm — the
same clamp helper #2543 introduced, now applied uniformly. New tests in
the same module assert the huge-finite case clamps for `Ball` (`radius`)
and `Capsule`/`Cylinder` (`half_height`/`radius`) to the sane ceiling.
Also landed in `06f86742`. `byroredux-physics`'s 148-test suite is green.
Issue #3238 remains open on GitHub — same recommendation as above.

### SAFE-D4-2026-08-23-01 (Fix #3239) — CONFIRMED RESOLVED

`e5329d64` ("Fix #3239: label two unsafe blocks with the SAFETY:
convention") added the missing labels at
`scene_buffer/descriptors.rs:249` and `scene_buffer/buffers.rs:917`.
Verified both sites now carry `SAFETY:`-prefixed comments.

### SAFE-D6-NEW-01 (`bindings.glsl` stale 348 B comment) — RESOLVED

`crates/renderer/shaders/include/bindings.glsl:99,107-108` now reads
"Mirrors the Rust `GpuMaterial` (364 B std430)" and cites
`gpu_material_size_is_364_bytes` by its current name. No longer stale.

### SAFE-D6-NEW-03 (`material.rs:67` stale test-file cross-reference) — RESOLVED

`crates/renderer/src/vulkan/material.rs`'s doc comment now names
`scene_buffer/shader_contract_tests.rs` (the file the `ui.vert`
non-mirroring test actually lives in post-split), not the retired
`gpu_instance_layout_tests.rs` name.

### SAFE-D6-NEW-02 (Existing: #2483, recurred) — STILL PRESENT

- **Severity**: LOW
- **Dimension**: 6 (R1 Material Table Layout — doc-comment accuracy only,
  zero runtime effect)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:173,176`
- **Description**: `MAX_MATERIALS`'s doc comment still reads "16384 × 348 B
  ≈ 5.7 MB per frame … The per-material size is pinned by
  `gpu_material_size_is_348_bytes`" — both the byte figure and the cited
  test name are stale. `GpuMaterial` is 364 B (pinned by
  `gpu_material_size_is_364_bytes`, confirmed via
  `crates/renderer/src/vulkan/material.rs:1421-1423`); the correct budget
  figure is 16384 × 364 B ≈ 5.96 MB per frame (≈ 11.9 MB at
  `MAX_FRAMES_IN_FLIGHT = 2`). This is the same doc site #2483 partially
  fixed (300 B → 348 B) before the struct grew again to 364 B — the
  underlying pattern (a load-bearing byte-budget comment not co-updated
  with the pinned-size test when the struct grows) is the same one
  #2483 was filed to close, and it recurred a second time.
- **Impact**: None on runtime correctness — this is a comment, and the
  actual budget (11.9 MB) is still comfortably inside the ~4 GB VRAM
  ceiling either way. Purely a doc-rot / maintainability item, but one
  that misleads anyone using this comment as ground truth for capacity
  planning.
- **Suggested Fix**: Update both numbers (348→364 B, 5.7→5.96 MB) and the
  cited test name in the same edit that touches
  `gpu_material_size_is_364_bytes` next time, or add a
  `debug_assert!`/doctest that fails loudly when the comment's hard-coded
  byte figure drifts from `size_of::<GpuMaterial>()`, closing the
  recurrence loop structurally instead of relying on manual doc upkeep.

## New Code Reviewed This Session (no findings)

Files changed `5db3b0b9..HEAD` were read in full or diffed and checked
against this domain's dimensions; none produced a new finding beyond
SAFE-BUILD-2026-08-24-01 above.

- **GPU morph-target blending, Phase D (#3231)** —
  `crates/renderer/src/vulkan/morph_compute.rs` (new `MorphSlot`: delta +
  weight buffers, `SHADER_DEVICE_ADDRESS` usage) and its lifecycle wiring
  in `crates/renderer/src/vulkan/context/{mod,init,teardown,draw,resize,
  skinned_blas_refit}.rs`. This is genuinely new GPU-resource-owning code
  and got the closest scrutiny of anything in this diff:
  - `MorphSlot::create`'s two `get_buffer_device_address` calls carry a
    correct SAFETY comment (both buffers just created with
    `SHADER_DEVICE_ADDRESS`, live for the call).
  - Bulk teardown: `context/teardown.rs:52-55` drains and destroys every
    `morph_slots` entry — same shape as `skin_slots`' bulk `Drop` loop.
  - Per-entity eviction: `pending_morph_unload_victims` is populated by
    `byroredux/src/cell_loader/unload.rs:263` on cell unload (mirroring
    `pending_skin_unload_victims` at `:254`) and drained in the same
    post-fence-wait eviction pass as skin slots
    (`skinned_blas_refit.rs:775-800`), with its own idle-timeout sweep
    using the same `min_idle = MAX_FRAMES_IN_FLIGHT + 1` threshold. No
    separate leak class opened — this is the Dimension-3 leak-guard
    pattern applied correctly to a new resource type, not a new gap.
  - Import-time guard: `crates/nif/src/import/mesh/morph.rs` caps target
    count at `MAX_MORPH_TARGETS_PER_MESH` (warn + truncate past it) and
    drops any individual morph target whose delta-vector length doesn't
    match `vertex_count` (warn + skip) — both release-mode-live guards
    upstream of the `debug_assert_eq!` in `MorphSlot::create`, which is
    therefore genuinely a second layer, not the only guard (the class of
    gap Dimension 9 asks auditors to check for).
- **Quest-fragment / trigger / quest-stage growth** (`crates/scripting/src/
  {fragment,trigger,quest_stages}.rs`, `translate/effects.rs`,
  `translate/recognizers/quest_stage_gate.rs`, `scene/quest_alias.rs`,
  `package.rs`) — ~1500 new lines across quest-fragment dispatch,
  actor-gated triggers (`OnTriggerEnterEvent` grew `triggerer: EntityId`
  → `triggerers: Vec<EntityId>` to preserve every actor entering a volume
  same-frame), and a new `QuestAliasReadinessGateRegistry` +
  `quest_alias_readiness_stage_system`. Checked for the two failure
  classes this domain cares about: unbounded growth and transient-marker
  leaks. `QuestAliasReadinessGateRegistry` is keyed by quest and updates
  in place (`install_quest_alias_readiness_gate`), bounded by distinct
  quest count like the existing `QuestDefinitionRegistry`. The new
  `QuestStageAdvancedBatch` writer sites (`fragment.rs`, `quest_stages.rs`)
  all feed the pre-existing `drain_component::<QuestStageAdvancedBatch>`
  call already wired into `cleanup.rs:94` — no new transient-marker
  leak. None of the new `translate/` lowering functions recurse into
  themselves (they walk an already-parsed, non-recursive `Effect`/`Stmt`
  slice); recursion-depth risk in the underlying Papyrus AST parser
  itself is `/audit-scripting` territory, not touched by this diff.
- **Save-format validation (M45)** — `crates/save/src/{driver,registry}.rs`
  add `validate_snapshot_types` (a new non-mutating typed-decode pass over
  every registered component/resource column) called from
  `restore_world` **before** `world.clear_entities()`. This is a
  safety *improvement*: it makes a malformed/incompatible snapshot's
  typed-decode failure transactional (caught before any live-world
  mutation) rather than leaving a partially-overlaid session on a
  mid-restore serde error. No leak or unsafe surface introduced —
  `ValidateFn` is a plain `Box<dyn Fn(Value) -> Result<(), SaveError>>`,
  same shape as the existing `SaveFn`/`LoadFn`. `byroredux-save`'s 26+14
  tests are green.
- **`crates/ui/src/navigator.rs`** (+47 lines) — the one `unsafe`-looking
  grep hit here is a log message ("`unsafe Scaleform archive path
  resolved from URL`"), not an `unsafe` block; false positive, no actual
  unsafe code added. `crates/ui/src/avm2_host.rs` (+178 lines) adds no
  `unsafe`.
- **`crates/debug-ui`** ("add player message display function") — a
  bounded, self-expiring `Option<(String, Instant)>` toast for save/load
  status, drawn through the existing egui `Frame`/`Area` machinery. CPU
  state only, no Vulkan handle, no growth risk (replaces, never
  accumulates).
- **`crates/renderer/src/vulkan/volumetrics.rs`** (+6/-2 lines) — pure
  `rustfmt` reformat of one ternary expression, no logic change.

## Regression Guards Re-Verified Intact (unchanged since 2026-08-23, spot-checked not re-derived)

- **Dim 1 (FFI)**: `fsr3-sys`'s `Context::create`/`Context::dispatch`
  still carry `# Safety` doc sections (`crates/fsr3-sys/src/lib.rs:365,
  379,403,408`); `crates/cxx-bridge/src/lib.rs` is still a 26-LOC
  no-raw-pointer placeholder (`native_hello() -> String` only).
- **Dim 2**: ECS cached-pointer contract, NIF `read_pod_vec` overflow
  guards (spot-checked `crates/nif/src/stream.rs:467` /
  `header.rs:382` — both still the sealed-`AnyBitPattern` pattern),
  `sfmaterial::BuiltinType::from_u32`'s checked match,
  `pex::OpCode::from_u8`'s range-check-before-transmute,
  `MAX_NIF_NODE_DEPTH`/`MAX_COLLISION_SHAPE_DEPTH` recursion bounds — all
  unchanged and confirmed by this session's `byroredux-nif`/`-pex`/
  `-sfmaterial` green test runs.
- **Dim 3**: `AllocatorResource` drop-ordering (`byroredux/src/
  app_events.rs`), deferred-destroy drain timing, `MaterialTable`/
  `AnimationClipRegistry` bounded growth — unchanged; the new `MorphSlot`
  leak-guard class is reviewed above under "New Code Reviewed."
- **Dim 4**: full-workspace unsafe-block sweep re-run this session
  (~710 `unsafe {` block openers across `crates/`+`byroredux/src/`, a
  drift from the skill doc's cited ~760-token count, consistent with the
  doc's own "counts drift, recount" guidance). A crude
  comment-proximity heuristic flagged ~205 "no nearby SAFETY comment"
  hits; manual spot-checks (`mesh.rs:1189/1199`, `morph_compute.rs:149`)
  confirmed these are false positives from a too-narrow lookback window,
  not real gaps — consistent with #2692's finding that this whole class
  of check is a token-counting artifact, not a real shortfall. Not
  re-litigated as a finding per that closed work item.
- **Dim 5**: `cargo check --workspace` clean; SPIR-V reflection tests
  (`scene_descriptor_reflection_tests`, part of `byroredux-renderer`'s
  743 green tests) pass. No Vulkan device was available in this
  environment to run validation layers live — per the No-Speculative-
  Vulkan-Fixes rule, no barrier/layout claim is asserted here beyond what
  `cargo test` can see.
- **Dim 6**: `GpuMaterial` = 364 B (`gpu_material_size_is_364_bytes`),
  field-offset pin, `MAX_MATERIALS` cap+truncation lockstep,
  `material_id` CPU-bounds guarantee — all passing tests; see
  SAFE-D6-NEW-02 above for the one remaining doc-drift item.
- **Dim 7**: Glass-passthrough loop guard, `GLASS_RAY_BUDGET` lockstep,
  Frisvad basis, IOR interior fallback — unchanged, part of
  `byroredux-renderer`'s green suite (`shader_constants` tests included).
- **Dim 8**: FLT_MAX pose-fallback sentinel, `AnimationClipRegistry`
  case-insensitive dedup, `SkinSlotPool` overflow guard — unchanged;
  `render/bone_palette_overflow_tests.rs` part of the green `byroredux`
  binary-crate suite (1515 passed).
- **Dim 9**: Material NaN-sentinel + `resolve_pbr()` contract,
  `BhkMultiSphereShape`/`BhkConvexListShape` finiteness — unchanged; the
  new morph-target import path (reviewed above) follows the same
  finite-at-the-boundary discipline.
- **Dim 10**: `EguiPass` teardown ordering, one-frame-deferred texture
  free, shared-queue-mutex scoping — unchanged; the new player-message
  toast is CPU-only and doesn't touch this surface.
- **Dim 11**: `crates/mod-runtime` — no-WASI-by-default, capability
  gating, per-instance isolation, fuel/stack ceiling — unchanged, zero
  `unsafe`, 17 tests green.

## Prioritized Fix Order

1. **SAFE-BUILD-2026-08-24-01** (HIGH) — restores `cargo test` /
   `cargo test --workspace` to a working state; trivial 3-arm fix,
   highest leverage-to-effort ratio of anything in this report.
2. Close #3237 and #3238 on GitHub — both are verified fixed in the
   working tree as of `06f86742`; no code action needed, just triage.
3. SAFE-D6-NEW-02 (LOW, Existing #2483 recurred) — doc-only, bundle with
   next doc pass; consider the structural fix (assert byte figure against
   `size_of` at compile/test time) to stop the recurrence for good.

Suggest: `/audit-publish docs/audits/AUDIT_SAFETY_2026-08-24.md`
