# Safety Audit — 2026-08-23

Scope: all 11 dimensions. Preset: `nif-deep` (part of
`/audit-suite --preset nif-deep`).

## Executive Summary

**2 NEW HIGH findings, 4 LOW findings, 0 CRITICAL.** Both HIGH findings are
real, previously-uncaught defects: one is a long-standing gap (unbounded ESM
GRUP-tree recursion, previously noted in a subsystem audit but never filed as
an issue) now correctly escalated under this domain's shared severity scale;
the other generalizes an already-fixed pattern (#2543's Cuboid shape-extent
clamp) to three sibling shape types that never received the same fix. All 4
LOW findings are doc-comment drift with zero runtime effect. Every previously
tracked regression guard was re-verified against current HEAD (not trusted
from the prior 2026-08-20 report) and remains intact.

⚠️ **Both HIGH findings involve untrusted/corrupt content reaching an
unguarded code path with no `Result`-typed recovery:**
- **SAFE-D2-2026-08-23-01** — a crafted `.esm`/`.esp` can stack-overflow and
  abort the whole engine process (no depth cap on GRUP-tree recursion).
- **SAFE-D9-2026-08-23-01** — a corrupt-but-finite Ball/Capsule/Cylinder
  collision shape reaches Rapier's broadphase unbounded, poisoning
  scene-wide contact detection (same failure class #2543 already fixed for
  Cuboid, not generalized to siblings).

## Findings

### SAFE-D2-2026-08-23-01 — ESM/ESP GRUP-tree recursion has no depth bound (HIGH)

- **Dimension**: 2 (Memory Corruption/UB — stack-overflow recursion risk)
- **Location**: `crates/plugin/src/esm/records/grup_walker.rs` (`extract_records`,
  `extract_records_with_modl`, `extract_dial_with_info`,
  `extract_quest_dialogue_scene_tree_inner`); `crates/plugin/src/esm/cell/wrld.rs`
  (`parse_wrld_children`, group types 4/5/6); `crates/plugin/src/esm/cell/walkers.rs`
  (`parse_cell_group`, group types 2/3)
- **Status**: Previously identified in `docs/audits/AUDIT_ESM_2026-08-13.md`
  as ESM-D1-04 (filed there as MEDIUM under that report's per-record-domain
  scale) but **never converted to a GitHub issue** — still present verbatim.
  Treat this entry as the canonical tracking record going forward.
- **Description**: Every GRUP-tree walker recurses into nested groups
  unconditionally — no depth counter, no `MAX_..._DEPTH` constant, unlike the
  NIF importer's `MAX_NIF_NODE_DEPTH` (128, confirmed intact) or the
  collision-shape resolver's `MAX_COLLISION_SHAPE_DEPTH` (64, confirmed
  intact). A GRUP header is only 20-24 bytes and `group_content_end` trusts
  the file's own `total_size` with only a `saturating_sub` floor — nothing
  stops a crafted plugin from nesting minimal GRUPs recursively for as many
  levels as its byte budget allows. `extract_records`/`extract_records_with_modl`
  are reached from every major top-level GRUP dispatcher, so this hits every
  ESM/ESP a user loads, including third-party mod content.
- **Impact**: A malicious or merely corrupt plugin crashes the whole engine
  process via stack overflow during load — an uncatchable, unrecoverable
  abort, unlike every other malformed-input case in this parser (which
  returns `Result::Err` and lets the caller skip the plugin).
- **Severity Rationale**: The ESM audit rated this MEDIUM under its own
  per-record-domain scale; under `_audit-severity.md`'s shared decision tree
  this is a hard, uncatchable process abort from untrusted input with no
  recovery path — rated HIGH here.
- **Suggested Fix**: Thread a `depth: u32` through all six affected walkers
  and bail (skip the group, log a warning) past a shared
  `MAX_GRUP_NESTING_DEPTH` (e.g. 32-64; vanilla content nests 3-4 tiers deep
  at most) — mirroring `MAX_NIF_NODE_DEPTH`'s exact pattern.

### SAFE-D9-2026-08-23-01 — `Ball`/`Capsule`/`Cylinder` Rapier conversion has no upper-bound clamp (HIGH)

- **Dimension**: 9 (NIFAL Boundary — NaN/Inf / unbounded values reaching a
  live subsystem)
- **Location**: `crates/physics/src/convert.rs:205-268` (`flatten_to_parts`,
  the documented single choke point before Rapier), `Ball`/`Capsule`/`Cylinder`
  arms, contrasted with the `Cuboid` arm at `:208-244`
- **Description**: Issue #2543 (closed, HIGH) fixed this exact choke point
  for `CollisionShape::Cuboid`: an astronomically large but finite half-extent
  used to reach `SharedShape::cuboid()` unbounded, handing Rapier's
  broadphase an effectively-infinite AABB overlapping the entire scene. The
  fix added `clamp_lane`, flooring non-finite lanes to `1e-3` and clamping
  finite lanes to `[1e-3, MAX_SANE_SHAPE_EXTENT]` (1,048,576.0), with a
  dedicated regression test. The sibling `Ball` (fed by `BhkSphereShape`/every
  sphere in `BhkMultiSphereShape`), `Capsule` (`BhkCapsuleShape`), and
  `Cylinder` (`BhkCylinderShape`) arms in the same `match` only apply
  `.max(1e-3)` — a floor inherited from before #2543 — with **no upper
  bound**. All four producers guard only *finiteness* at the NIF import
  boundary, the exact posture `Cuboid`'s producer had before #2543.
- **Impact**: A corrupt-but-finite radius/half-extent (e.g. `1e30`,
  corrupt-but-legal per IEEE 754) on any of these four shape types reaches
  Rapier's broadphase unbounded — the same all-scene-blast-radius failure
  #2543 was filed to close, just uncovered for three of four shape types.
  Once live, every other collider in the scene reports spurious contact
  pairs against it, corrupting physics-driven `Transform` updates that feed
  the per-frame GPU instance buffer scene-wide.
- **Suggested Fix**: Reuse `Cuboid`'s `clamp_lane` for `Ball`'s radius and
  both lanes of `Capsule`/`Cylinder`; add a
  `huge_finite_{ball,capsule,cylinder}_*_clamps_to_sane_ceiling` test per
  shape alongside the existing `Cuboid` test.

### LOW findings (4, all doc-comment drift, zero runtime effect)

| ID | Dim | Location | Issue |
|----|-----|----------|-------|
| SAFE-D4-2026-08-23-01 | 4 | `scene_buffer/descriptors.rs:249`, `scene_buffer/buffers.rs:917` | Two `unsafe` blocks carry correct safety reasoning in prose but skip the codebase's `SAFETY:` label convention (invariants independently verified sound at both sites) |
| SAFE-D6-NEW-01 | 6 | `crates/renderer/shaders/include/bindings.glsl:99,107-108` | Comment still cites retired `348 B`/`gpu_material_size_is_348_bytes` pin; struct is now 364 B (`#2221`), test renamed in lockstep on the Rust side |
| SAFE-D6-NEW-02 (Existing: #2483, recurred) | 6 | `scene_buffer/constants.rs:173,176` | `MAX_MATERIALS` doc still says `16384 × 348 B ≈ 5.7 MB`; correct is `≈ 5.96 MB` at 364 B — #2483's prior partial fix (300B→348B) was overtaken by the struct growing again |
| SAFE-D6-NEW-03 | 6 | `crates/renderer/src/vulkan/material.rs:67` | Cross-reference to `ui.vert` lockstep test still names `gpu_instance_layout_tests.rs`; test moved to `shader_contract_tests.rs` in the 2026-08-20 test-file split |

## Regression Guards Re-Verified Intact (not re-reported)

- **Dim 1 (FFI)**: `fsr3-sys` create/dispatch/Drop contracts sound at every
  real call site; Ruffle/wgpu boundary (`crates/ui/src/player.rs`) has no
  lifetime/teardown-ordering hazard vs. `VulkanContext`; cxx-bridge still a
  26-LOC no-pointer placeholder. (#2829 noted, out of Dim-1 scope — belongs
  to Dim 3's leak class.)
- **Dim 2**: ECS cached-pointer contract (#35/#1367), `GpuInstance` vec3-free
  layout across all 5 GLSL mirrors incl. the new #3231 morph fields, NIF
  `read_pod_vec` overflow guards, `sfmaterial::BuiltinType::from_u32`'s
  checked match, `pex::OpCode::from_u8`'s range-check-before-transmute, NIF
  scene-graph/collision-shape recursion bounds (`MAX_NIF_NODE_DEPTH=128`,
  `MAX_COLLISION_SHAPE_DEPTH=64`) — all confirmed intact (the last one is the
  reference shape SAFE-D2-2026-08-23-01 above shows ESM never received).
- **Dim 3**: Rapier cell-unload release (#1520/#1531), deferred-destroy drain
  timing, `AllocatorResource` drop ordering incl. panic-unwind path (#1406),
  `MaterialTable`/`AnimationClipRegistry` bounded growth.
- **Dim 4**: SAFE-2026-08-20-03 (6 comment-less blocks in `water.rs` +
  `predicates.rs`) confirmed **RESOLVED** since last report.
- **Dim 5**: vkCreate/vkDestroy pairing + reverse-order Drop (incl. the fresh
  #1749 init.rs/teardown.rs split), TLAS resize wait (#1390), volumetrics
  dispatch gate, SPIR-V reflection tests; the #3231 `GpuInstance` growth
  spot-audited (all 5 GLSL sites in lockstep, both construction sites zero
  the new buffer-address fields).
- **Dim 6**: `GpuMaterial` byte contract (364 B, offsets, flat-scalar
  discipline, zero-padding, `MAX_MATERIALS=16384` cap+truncation lockstep,
  `material_id` CPU-bounds guarantee, `ui.vert` non-mirroring) — all
  enforced by passing tests.
- **Dim 7**: Glass-passthrough loop guard (#789), `GLASS_RAY_BUDGET`
  lockstep, Frisvad basis (#820), IOR interior fallback, `DBG_VIZ_GLASS_PASSTHRU`
  uniqueness — all confirmed via 46/46 passing `shader_constants` tests.
- **Dim 8**: FLT_MAX pose-fallback sentinel (#772), `AnimationClipRegistry`
  case-insensitive dedup (#790, its non-shrinking `Vec` is a separate
  already-open leak, #2689), B-splines-reach-FNV/FO3 rule, Starfield
  walkability, `SkinSlotPool` overflow guard — all reconfirmed against the
  #2221/#3231-touched animation files.
- **Dim 9**: Material NaN-sentinel + `resolve_pbr()` contract across all
  production constructors and 7 spawn call sites; particle emitter
  finiteness/boundedness at extract + downstream re-check;
  `BhkMultiSphereShape`/`BhkConvexListShape` import-time finiteness +
  allocation bound via `check_alloc`.
- **Dim 10**: `EguiPass` teardown ordering (verified against the post-#1749
  split `teardown.rs`, first action in `Drop`), one-frame-deferred texture
  free (traced the dual-fence wait, confirmed sound), shared queue mutex
  scoped tightly around `set_textures` only (#1713).
- **Dim 11**: mod-runtime — no-WASI-by-default (verified at 3 levels incl.
  `Cargo.lock`), capability gating, per-instance isolation, `#3049`'s fuel/stack
  ceiling fix intact, `compile()` rejects malformed input cleanly (independently
  re-verified with a throwaway test). #3050/#3051/#3215 remain open and
  unchanged, not re-reported.

## Prioritized Fix Order

1. **SAFE-D2-2026-08-23-01** (HIGH) — untrusted-input DoS, affects every game/plugin load.
2. **SAFE-D9-2026-08-23-01** (HIGH) — scene-wide physics corruption from malformed collision content.
3. LOW doc-drift cleanup (SAFE-D4/D6 ×4) — no urgency, bundle into next doc pass.

Suggest: `/audit-publish docs/audits/AUDIT_SAFETY_2026-08-23.md`
