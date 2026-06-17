# Safety Audit — ByroRedux — 2026-06-16

**Scope**: `unsafe` blocks, memory/resource leaks, undefined behavior, Vulkan
spec violations. Full SKILL dimension sweep (1–10).

**Method**: grep the full `unsafe` surface across `crates/` + `byroredux/`,
verify every regression-guard premise against current code, dedup against
GitHub issues (`/tmp/audit/issues.json`, 400 issues) + prior `docs/audits/`.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 0 |

One NEW MEDIUM (batched unsafe-without-SAFETY-comment gap). Every CRITICAL/HIGH
regression guard in the SKILL was verified **intact** — recast as PASS below,
not as findings.

---

## Findings

### SAFE-2026-06-16-01: ~200 renderer `unsafe` blocks still lack SAFETY comments (outside the #1432-fixed files)
- **Severity**: MEDIUM
- **Dimension**: 4 — Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/` (worst offenders below)
- **Status**: NEW (distinct scope from CLOSED #1432 / SAFE-U6)
- **Description**: The renderer carries 616 `unsafe` tokens / 311 `SAFETY`
  mentions. Counting block-opening `unsafe {` forms only (non-test), 544 blocks
  exist and ~327 have no SAFETY comment within the preceding 8 lines. A portion
  of that 327 are false positives (a single per-function SAFETY comment covering
  several batched ash `cmd_*` calls), but spot-checking confirms a large genuine
  residue. The unified-severity Special Rules table sets "`unsafe` without safety
  comment = MEDIUM"; the SKILL directs reporting these as one batched finding.
- **Evidence**: Per-file `unsafe {` vs `SAFETY` counts (current tree):
  - `acceleration/blas_static.rs` — 32 uncommented (e.g. the `drain` destroy at
    line 117 mirrors the commented `tick` destroy at line 83 but carries no
    comment; `evict_unused_blas` call at line 163 uncommented)
  - `composite.rs` — 41 `unsafe {` / 17 SAFETY (e.g. lines 282, 306 — partial
    cleanup + image-create FFI, no per-block note)
  - `volumetrics.rs` — 27 / 4
  - `bloom.rs` — 21 uncommented
  - `buffer.rs` — 25 / 10
  - `context/draw.rs` — ~20 uncommented
  - `context/helpers.rs` — 16, `texture.rs` — 15, `context/mod.rs` — 15,
    `skin_compute.rs` — 13, `device.rs` — 13, `context/resize.rs` — 12,
    `texture_registry.rs` — 10, plus a long tail.
  - The two files named in CLOSED #1432 are now FULLY commented and must be
    excluded: `gpu_timers.rs` (29 `unsafe {` / 29 SAFETY), `blas_skinned.rs`
    (13 / 16). Commit `ec23ed1a` fixed exactly those two — #1432 was a targeted
    fix, not a blanket sweep, so the remaining files are a new, distinct gap.
- **Impact**: Documentation / maintainability debt, not a live soundness bug.
  None of the sampled blocks holds a FALSE invariant — they are ash
  create/destroy/dispatch FFI wrappers sound by the surrounding handle lifetime.
  Risk is that an uncommented block masks a future invariant break during a
  refactor (the higher-severity failure mode the comment policy guards against).
- **Related**: CLOSED #1432 (SAFE-U6), CLOSED #1403/#1408/#1415/#1416/#1425
  (prior targeted SAFETY-comment fixes).
- **Suggested Fix**: Continue the tiered policy #1432 established — trivial
  destroy/create calls get a one-line note, sync-dependent calls get a full
  comment. Prioritize `blas_static.rs`, `composite.rs`, `volumetrics.rs`,
  `bloom.rs`, `buffer.rs`, `draw.rs` (the highest-blast-radius files).

---

## Verified Intact (PASS — regression guards, no finding)

**Dimension 1 — FFI lifetime (cxx).** `crates/cxx-bridge/src/lib.rs` still
exposes exactly one bridge fn, `native_hello() -> String`. No `*const`, `&[u8]`,
`Box<…>`, or Rust-reference-taking `extern "C++"` fn. Scope guard holds — PASS.

**Dimension 2 — Memory corruption / UB.**
- Cached-pointer ECS contract (`crates/core/src/ecs/query.rs`): `QueryRead`,
  `QueryWrite`, `ComponentRef` cache a `*const`/`*mut` resolved in `new()` and
  deref under a held lock guard; each deref (lines 64, 135, 143, 289) has a
  SAFETY comment correctly tying validity to the pinned guard, and `&mut self`
  gates `storage_mut`. PASS (#35/#1367).
- NIF POD reads (`stream.rs:350` `read_pod_vec`, `header.rs:382` mirror): byte-
  count overflow guard (`checked_mul`) present, `T: AnyBitPattern` sealed bound,
  LE-host compile gate, correct SAFETY comment. `bs_geometry.rs:338-340`
  `AnyBitPattern` impls for `BoneWeight`/`Meshlet`/`CullData` are padding-free
  `#[repr(C)]` scalar aggregates (CPU-side parse structs, not std430 GPU
  uploads). PASS.
- sfmaterial `BuiltinType::from_u32` (`types.rs:37-55`) is a checked `match`
  with `_ => return Err(UnsupportedBuiltin { raw })`; no `transmute`. PASS.

**Dimension 3 — Leaks / drop ordering.**
- `App::drop` (`byroredux/src/main.rs:456-463`) removes `AllocatorResource`
  BEFORE `renderer.take()`, structural on every teardown incl. panic unwind
  (#1406/#1477/#1640). PASS.
- Deferred-destroy tick runs AFTER `wait_for_fences` (`context/draw.rs:580+`);
  `deferred_destroy.rs::drain` provides the shutdown sweep (#418/#732). PASS.
- Rapier release wired + `rapier_release_tests.rs` asserts body/collider/joint
  emptiness post-unload (#1520/#1531). PASS.
- BLAS scratch shrink on cell unload (`cell_loader/unload.rs:133`) — correct
  SAFETY comment (main thread, no in-flight build). PASS.

**Dimension 5 — Vulkan spec.** TLAS resize calls `device.device_wait_idle()`
before freeing old allocation (`acceleration/tlas.rs:322`, #1390). PASS.
Validation-layer / RenderDoc-only barrier-layout claims were NOT asserted (the
engine was not run with validation layers in this pass — none reported as bugs,
per the No-Speculative-Vulkan-Fixes rule).

**Dimension 6 — R1 material layout.** `gpu_material_size_is_300_bytes`
(material.rs:1199) asserts 300 B; `gpu_material_field_offsets_match_shader_contract`
(1342) pins per-field offsets; intern caps at `MAX_MATERIALS = 16384`
(material.rs:1097), `upload_materials` debug_asserts + `.min(MAX_MATERIALS)`
clamps (upload.rs:540-545). Lockstep intact. PASS.

**Dimension 7 — IOR/glass.** Glass passthru gated by `REFRACT_PASSTHRU_BUDGET`
+ materialKind check (`triangle.frag:1292-1344`), `GLASS_RAY_BUDGET = 1048576`
(`shader_constants_data.rs:59`), Frisvad basis present
(`math_common.glsl`/`triangle.frag`). PASS.

**Dimension 8 — NPC/anim spawn.** Bone-palette overflow guarded:
`.take(MAX_BONES_PER_MESH)` per entity (`render/skinned.rs:158-159`),
`.min(MAX_TOTAL_BONES)` + debug_assert upload-side (`scene_buffer/upload.rs:184,342`),
`bone_palette_overflow_tests.rs` asserts entities drop rather than over-index.
PASS.

**Dimension 9 — NIFAL NaN boundary.** `translate_material` seeds `f32::NAN`
(material_translate.rs:157-158) then calls `material.resolve_pbr()` (line 160);
`resolve_pbr` (`components/material.rs:638-656`) detects `is_nan()` and clamps
metalness∈[0,1], roughness∈[0.04,1.0]. Single boundary. PASS.

**Dimension 10 — debug-ui teardown.** Not re-flagged; no `unsafe` in
`crates/debug-ui/src`. egui teardown ordering is covered by CLOSED
#1421/#1433/#1483/#1491 (verified not regressed in title scan).

---

## Deduplication Notes

- `/tmp/audit/issues.json` — 400 issues fetched. No OPEN issue matches
  `unsafe`/`SAFETY`. The only OPEN safety-adjacent issue is #1316 (condition-
  evaluator stub branches), out of this domain's scope.
- All safety regression guards named in the SKILL trace to CLOSED issues whose
  fixes are confirmed present (see PASS section); none regressed.
