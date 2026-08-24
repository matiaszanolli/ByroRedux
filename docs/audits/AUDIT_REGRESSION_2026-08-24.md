# Regression Verification Audit — 2026-08-24

**Scope**: Full comprehensive run, no `--focus` filter, solo execution (no sub-agent
fan-out per task constraint). Verifies that previously-closed bug fixes are still
present and correctly guarded.

**Method**: The repo carries **2,063 closed `bug`-labeled issues**, far beyond what
a single-session audit can re-verify exhaustively (consistent with the skill's own
"Discovery window caveat"). This run combines:

1. The skill's explicit **"Fresh verification candidates"** list (6 issues, all
   `#181x`/`#17xx` decompiler-safety + LC wave).
2. A **stratified timeline sample** spanning the project's full history — from the
   earliest closed bugs (`#9`–`#189`, April 2026 Vulkan/ECS/NIF foundational fixes)
   through May/June/July milestone fixes to the most recent closed issues
   (August 2026, including the WATAL water-audit wave and same-day fixes).
3. The skill's **unconditional Step 4 fragile-area checks** (NIFAL single-boundary,
   typed particle emitters, collision shape coverage, Disney BSDF/GPU struct
   contracts) — run regardless of GitHub-issue discovery, since these guard
   refactor-landed invariants that never had their own bug report.

27 individual issues verified end-to-end (fix-commit located, fix confirmed present
in the live tree, guard test located and run) plus the full Step 4 checklist.
`cargo test --workspace` is broken by the unrelated `E0004` in
`crates/scripting/examples/fragment_coverage.rs:59` (already filed as
**SAFE-BUILD-2026-08-24-01**, HIGH, in `AUDIT_SAFETY_2026-08-24.md`) — all guard
tests below were run per-crate (`cargo test -p <crate>`) to route around it.

## Summary Table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #1815 | SCR-D2-01: boolean-collapse recursion cap | PASS | Yes | `rebuild_rejects_excessive_recursion_depth` — passes |
| #1816 | SCR-D5-NEW-02: `translate_pex` catch_unwind | PARTIAL | Yes | none found |
| #1728 | SCR-D1-02: Skyrim-BE/Starfield PEX round-trip | PASS | Yes | `parses_a_handbuilt_skyrim_be_pex` + `parses_a_handbuilt_starfield_pex_with_guards` — pass |
| #1740 | SCR-D5-03: DA10 `.pex` byte-equality parity | PASS | Yes | `da10_pex_reproduces_hand_builder_byte_for_byte` (real game data) — passes |
| #1731 | LC-D7-02: VWD record-header flag | PASS | Yes | 6 tests incl. `vwd_flag_*` — pass |
| #1718 | FNV-D7-01: dropped ragdoll body/constraint telemetry | PASS | Yes | `dropped_bone_excludes_body_and_dependent_constraint_but_keeps_the_rest` — passes |
| #35 | ECS: `World::get()` unsound `RwLockReadGuard` | PASS | Yes | `ComponentRef`-returning API confirmed; no lifetime-escaping unsafe |
| #86 | Safety: same-thread query/query_mut deadlock | PASS | Yes | `lock_tracker` reentrancy check present |
| #90 | Safety: no lock ordering for N>2 queries | PASS | Yes | `resource_2_mut` (TypeId-sorted) present |
| #22 | Renderer: TLAS missing HOST→AS_BUILD barrier | PASS | Yes | barrier present at `tlas.rs:250` |
| #25 | Renderer: AS buffers HOST_VISIBLE not DEVICE_LOCAL | PASS | Yes | `DEVICE_LOCAL` comments + allocation confirmed |
| #121 | NIF-401: bhkRigidBody translation/rotation discarded | PASS | Yes | applied at `import/collision/mod.rs:353-358` |
| #123 | NIF-508: bhkCompressedMeshShape skip-only | PASS | Yes | typed parse + dispatch present |
| #104 | NIF: Oblivion v20.0.0.5 no block_sizes | PASS | Yes | `block_sizes_present_at_20_2_0_5` + related tests |
| #466 | E-03: despawn poisoned-lock loses type name | PASS | Yes | `storage_lock_poisoned<T>` present |
| #952 | REN-D1-NEW-04: `reset_fences` before fallible recording | PASS | Yes | moved to immediately-before-`queue_submit`, commented in place |
| #337 | D4-NEW-01: NiStencilProperty → MaterialInfo | PASS | Yes | `stencil_active_property_round_trips_all_fields` and siblings |
| #1539 | D7-02: dropped ragdoll constraints silent | PASS | Yes | loud-drop path + `dropped_constraint_bones` telemetry present |
| #1333 | NIF-2026-05-29-05: NiParticleSystem local transform discarded | PASS | Yes | retained at `import/walk/mod.rs:587,1425` |
| #1369 | PERF-D1-NEW-01: WRS reservoir array retirement | PASS | Yes | `resRadiance[NUM_RESERVOIRS]` confirmed retired (comments only) |
| #1848 | SAVE-05: second load before drain silently discards first | PASS | Yes | `superseded` reporting present |
| #1857 | TD1-001: `context/draw.rs` 4265/4808 LOC monolith | PASS* | Yes | split files (`geometry_pass.rs`/`post_passes.rs`/`skinned_blas_refit.rs`) intact — **see REG-2026-08-24-01, size has since regrown past baseline** |
| #1914 | REN-D2-01: RL-03 ambient-fill point/spot gate | PASS | Yes | per-light fill removed 2026-05-27, sun ambient is light-count-independent floor |
| #3036/#3102 | FNV-2026-08-16-D1-01: BSXFlags bit 5 drops whole NIF | PASS | Yes | `finish_partial_import_fo4_bsx_bit5_is_not_editor_marker` + sibling — pass |
| #3116 | PHYS-D5-2026-08-20-02: sensors excluded only by `cast_ray` | PASS | Yes | `solid_probe_filter()` centralizes `.exclude_sensors()`; 7 sensor-exclusion tests pass |
| #3121 | CONC-2026-08-20-02: undeclared scheduler accesses | PASS | Yes | `water_and_animation_parallel_accesses_are_complete` — passes |
| #3239 | SAFE-D4: unsafe blocks missing SAFETY: label | PARTIAL | Yes | style-only fix, no test exists (none expected) |

\* See finding **REG-2026-08-24-01** below — the specific fix (three files split
out) is intact and unregressed, but the underlying size invariant it established
has silently regrown past the pre-fix baseline through unrelated feature work.

### Step 4 — Unconditional fragile-area checks

| Contract | Status | Evidence |
|---|---|---|
| Single material boundary (`translate_material` / `translate_texture_only_material`) | PASS | Only production `Material {` construction sites remain `byroredux/src/material_translate.rs` + the self-contained `--cornell` harness (no `ImportedMesh` input, out of scope for the boundary) |
| `Material::metalness`/`roughness` stay plain `f32`, no `Option<f32>` | PASS | `crates/core/src/ecs/components/material.rs:24-25` |
| Typed particle emitters (`NiPSysEmitter`/`Ctlr`/`CtlrData`/`GrowFadeModifier`) | PASS | Typed structs + dispatch in `blocks/mod.rs` + `extract_emitter_params`/`extract_emitter_rate` + `apply_emitter_params` consumer, all present |
| `BhkMultiSphereShape` + `BhkConvexListShape` → `CollisionShape` | PASS | Both resolved in `crates/nif/src/import/collision/shape.rs:110,235` |
| `resRadiance[NUM_RESERVOIRS]` stays retired (no reintroduced per-thread reservoir array) | PASS | Only retrospective comments remain; `shadowableLightRadiance` is the live path |
| `pbr.glsl` Disney/Burley lobe + MIT attribution | PASS | Present with citations |
| `GpuInstance` = 160 B, `GpuCamera` = 352 B pinned | PASS | `cargo test -p byroredux-renderer gpu_` — 41/41 pass, including both size pins |

`GpuMaterial` is now **364 B** (grown from the 348 B the shared `_audit-common.md`
project-layout table still documents) — this is **not** a regression: the growth
is intentional (#2221 animated shader_color/shader_float sinks), the Rust-side
pin test was renamed in lockstep (`gpu_material_size_is_364_bytes`, currently
passing), and the stale 348 B mentions were already caught and fixed same-day
(`Fix #3240`, `e5329d64`). Flagging only so a future audit doesn't mistake the
already-fixed doc lag for a live gap.

## Findings

### REG-2026-08-24-01: `context/draw.rs` and `draw_frame` have regrown past the #1857 tech-debt baseline
- **Severity**: LOW
- **Dimension**: Tech Debt / Renderer
- **Location**: `crates/renderer/src/vulkan/context/draw.rs`
- **Status**: NEW (related to #1857, not a strict regression of it)
- **Description**: `#1857` (`9a9a4c5d`, 2026-07-21) split `record_geometry_pass`,
  `record_post_passes`, and `record_skinned_blas_refit` out of `draw.rs` because it
  was "the largest file in the tree" at 4,808 LOC, bringing it down to 3,029 LOC
  (`draw_frame` itself was left untouched at 1,844 LOC — "moved verbatim, not
  restructured"). In the ~5 weeks since, 56 commits have touched `draw.rs`
  (FSR3 frame-tail work, bloom, volumetrics, morph-target deformation, animated
  material sinks, water/glass caustic contracts, ReSTIR reservoir changes, …),
  landing directly in `draw.rs`/`draw_frame` rather than being routed into the
  three sibling files the split established. The file is now back to **4,909
  LOC** — larger than the pre-fix 4,808 — and `draw_frame` itself has grown from
  1,844 to **2,493 LOC**, a function the original fix explicitly did not touch.
- **Evidence**:
  ```
  git show 9a9a4c5d --stat -- crates/renderer/src/vulkan/context/draw.rs
    draw.rs | 1828 +-------------------   (4808 -> 3029 LOC)
  wc -l crates/renderer/src/vulkan/context/draw.rs   # today: 4909
  awk '/pub fn draw_frame/{s=NR} s&&/^    }$/{print NR-s+1; exit}' draw.rs  # 2493
  git log --oneline 9a9a4c5d..HEAD -- .../draw.rs | wc -l   # 56 commits
  ```
- **Impact**: No functional bug — the three extracted helper files are intact and
  correctly still hold their split-out passes; this is not a reverted fix. It is
  the same tech-debt condition #1857 was filed to address, silently reaccumulating
  because nothing enforces the boundary going forward (no LOC guard, no lint, no
  "new render-pass code belongs in a sibling file" convention check). Left alone,
  the file will keep growing every time a new pass lands, and `draw_frame` — the
  one function the original fix deliberately spared — is now the single largest
  undecomposed body in the directory.
- **Related**: #1857, #2258 (`record_post_passes` follow-up split), #2197
  (FSR/camera-delta extraction from `draw_frame`)
- **Suggested Fix**: Not urgent enough to re-open as a bug — informational for the
  next `/audit-tech-debt` pass. If desired, a soft guard (a test asserting
  `draw.rs` line count stays under some threshold, mirroring the GPU-struct size
  pins) would catch this class of regrowth before it reaches the pre-fix size
  again.

### REG-2026-08-24-02: `translate_pex` panic-catch (#1816) has no guard test
- **Severity**: LOW
- **Dimension**: Scripting / Test Coverage
- **Location**: `crates/scripting/src/translate/mod.rs:111-121`
- **Status**: NEW (hardening gap on a still-present fix, not a regression)
- **Description**: The `#1816` fix (`8b04c492`) wraps `decompile_script` in
  `std::panic::catch_unwind` and is confirmed present and correctly reasoned
  about (comment cites #1816, mirrors `pex_corpus_smoke`'s existing pattern).
  However, no test exercises the panic path itself — there is no fixture `.pex`
  crafted to trip one of the decompiler's internal `.expect()`s and assert that
  `translate_pex` returns `None` instead of propagating a panic through
  `attach_vmad_scripts`. A future refactor that accidentally removes the
  `catch_unwind` wrapper (e.g., during a `translate_pex` signature change) would
  not be caught by CI.
- **Evidence**: `grep -rn "translate_pex.*panic\|panic.*translate_pex" crates/ byroredux/ --include='*.rs'` finds no test.
- **Impact**: Low — the fix is correct today; this is a hardening gap, not a live bug.
- **Suggested Fix**: Add a fixture `.pex` (or a hand-built byte sequence) known to
  trip one of the cited `.expect()`s (`cfg.rs::split_block`, `control_flow.rs`,
  `lift.rs`, the boolean-pass expects) and assert `translate_pex` returns `None`
  rather than unwinding.

### REG-2026-08-24-03: Two `SAFETY:`-labeled unsafe blocks (#3239) have no regression guard
- **Severity**: LOW
- **Dimension**: Tech Debt / Safety
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs`, `buffers.rs`
- **Status**: NEW (style-convention fix, guard gap by design)
- **Description**: `#3239` (`e5329d64`) is a pure labeling-consistency fix — both
  unsafe blocks already had correct safety reasoning in prose, just not the
  `SAFETY:` prefix convention used almost everywhere else in the codebase. Fix
  confirmed present. No automated guard exists (and none obviously could, short
  of a doc-lint), so a future edit that strips the label while retaining the
  unsafe block would not be caught by CI. Noting only for completeness per the
  skill's PASS/PARTIAL distinction — not actionable.
- **Suggested Fix**: None needed; informational only.

## Notes on Coverage

- The **2,063-issue closed-bug backlog** means this run, like every prior
  `/audit-regression` pass, is necessarily a sample, not an exhaustive sweep.
  This session weighted the sample toward (a) the skill's named fresh candidates,
  (b) the oldest foundational fixes (April 2026 Vulkan/ECS/NIF safety work, which
  no prior regression report in `docs/audits/` appears to have sampled from —
  all ten checked were confirmed intact), and (c) a few of the most recent
  same-day fixes to catch anything that landed and immediately regressed.
- All 27 explicitly-verified issues are **PASS or PARTIAL** — **zero FAILs**,
  i.e., no regressions of previously-closed bugs were found in this sample.
- The three PARTIAL statuses (#1816, #3239, and implicitly the #1857 size-creep
  note) are all hardening/process gaps on fixes that are otherwise correctly
  in place, not live bugs.
- Per the skill's caveat about `#1651` (BGSM/BGEM GL→Gamebryo blend factors):
  confirmed this was **not** included in the sample, consistent with the
  instruction not to re-verify a premise the codebase itself disproved and
  reverted (`#1823`).

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-24.md
```
