---
description: "Verify closed bug fixes haven't regressed — dynamically discovers and checks"
argument-hint: "--issues <N,N,N> --limit <N>"
---

# Regression Verification Audit

Verify that ALL previously fixed issues have not regressed. Dynamically discovers closed bug issues and verifies their fixes are still in place.

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--issues <N,N,N>`: Only verify specific issue numbers (e.g., `--issues 1,2,9`)
- `--limit <N>`: Maximum number of closed issues to verify (default: 50)
- `--label <label>`: Filter issues by label (default: `bug`)

## Step 1: Discover Fixed Issues

Fetch closed bug issues from GitHub:

```bash
gh issue list --repo matiaszanolli/ByroRedux --state closed --label bug --limit 50 --json number,title,body,closedAt,labels
```

If `--issues` is provided, fetch only those specific issues instead.

> **Default-window caveat.** The repo now has 1200+ closed issues (past #1295). The default `--limit 50` window only covers the most-recently-closed bugs, so older high-value fixes — and the recent NIFAL / Disney-BSDF / water-caustics closure wave (e.g. #1210, #1248–#1257) — get **no coverage** unless you raise `--limit` or pass them explicitly via `--issues`. When auditing the translation/shader tier, either bump `--limit` or add those issue numbers to `--issues`. The unconditional **NIFAL canonical-translation fragile-area checks in Step 3** are the safety net for refactor-landed (never-an-issue) regressions.

For each closed issue, extract:
- **Issue number and title**
- **File references** from the body (look for backtick-quoted paths like `crates/nif/...`)
- **Fix description** from the body (look for "Acceptance Criteria", fix commits)
- **Related issues** (look for `#NNNN` references)

## Step 2: Verify Each Fix

### Step 2a: Locate the Fix
1. Search for the issue number in commit messages: `git log --oneline --grep="#<NUMBER>"`
2. If found, check the diff: `git show <commit> --stat`
3. Read the referenced file(s) to confirm the fix is present

### Step 2b: Check for Regression Tests
1. Search for test files referencing the issue: `grep -r "<NUMBER>" crates/ --include="*.rs" -l`
2. Look for test names: `grep -r "test.*<keyword>" crates/ --include="*.rs"`
3. Record what tests exist

### Step 2c: Assign Status
- **PASS**: Fix code confirmed present + regression tests exist
- **PARTIAL**: Fix code confirmed present but NO regression tests
- **FAIL**: Fix code is missing or broken (REGRESSION DETECTED)
- **UNVERIFIABLE**: Cannot determine fix location from issue body

## Step 3: Special Checks

For ByroRedux-specific fragile areas:
- **Depth bias** (#16 and decal fixes): Verify bias values still applied for decal meshes (`RenderLayer::depth_bias()` in `crates/core/src/ecs/components/render_layer.rs`, consumed in `crates/renderer/src/vulkan/context/draw.rs`)
- **TLAS descriptor** (validation fixes): Verify `write_tlas` called at init for all frames (`crates/renderer/src/vulkan/scene_buffer/descriptors.rs`)
- **NiBoolInterpolator** (parse fix): Verify `read_byte_bool` not `read_bool` (`crates/nif/src/stream.rs` + `crates/nif/src/blocks/interpolator.rs`)
- **Name collision** (#9): Verify `root_entity` scoping still in `AnimationPlayer` (`crates/core/src/animation/player.rs`)
- **XCLL parsing** (cell lighting): Verify byte offsets for directional rotation (`crates/plugin/src/esm/cell/walkers.rs`; canonical size sets pinned per-game era)

### NIFAL canonical-translation fragile areas

These guard the NIF→canonical translation tier (spec: `docs/engine/nifal.md`). Regressions here are **invisible to GitHub-issue discovery** — most landed as proactive refactors, not closed bugs — so they must be checked unconditionally. **See also `/audit-nifal`** for the dimension-level checklist of this layer.

- **Single material boundary** (NIFAL): Verify `byroredux/src/material_translate.rs::translate_material` is still the *only* `ImportedMesh → Material` site (per-game material classification lives here, never in the shader), and that `Material.metalness` / `Material.roughness` (`crates/core/src/ecs/components/material.rs`) stay plain resolved `f32` — no reintroduced `Option<f32>` or render-time `classify_pbr`. The resolve-once contract is `*_override.unwrap_or(f32::NAN)` at the boundary + `Material::resolve_pbr` (calls `classify_pbr_keyword`) filling only the NaN slots.
- **Typed particle emitter dispatch** (NIFAL): Verify `NiPSysEmitter` / `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` still parse as **typed** blocks (`crates/nif/src/blocks/particle.rs`, dispatched in `crates/nif/src/blocks/mod.rs`), feed `extract_emitter_params` / `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs` → `ImportedEmitterParams` in `crates/nif/src/import/types.rs`), and that `byroredux/src/systems/particle.rs::apply_emitter_params` consumes them — not a silent regression to opaque `NiPSysBlock`. The regression signature is a zero-sized emitter or a clobbered preset color.
- **Collision shape coverage** (NIFAL): Verify `BhkMultiSphereShape` + `BhkConvexListShape` still translate to a `CollisionShape` in `crates/nif/src/import/collision.rs` (they were previously dropped); a regression silently drops the shape back to `None`.
- **Disney BSDF + GPU struct contracts** (recent shader wave): Verify `crates/renderer/shaders/triangle.frag` still carries the Disney/Burley lobe + the GLSL-PathTracer MIT attribution block (top-of-file, Burley 2012 SIGGRAPH cite), `NUM_RESERVOIRS = 16` is intact, and the `GpuCamera` size contract holds (304 B — guard test `gpu_camera_is_288_bytes` in `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`; note the function name is stale, the asserted value is 304). `GpuInstance` must also stay 112 B (`gpu_instance_is_112_bytes_std430_compatible`).

## Output

Write to: **`docs/audits/AUDIT_REGRESSION_<TODAY>.md`**

### Per-Issue Format
```
## #<ISSUE>: <Title>
- **Status**: PASS | PARTIAL | FAIL | UNVERIFIABLE
- **Closed**: <date>
- **Fix commit**: <hash> (or "not found")
- **File checked**: `<path>:<line>`
- **Fix present**: Yes / No / Unknown
- **Tests exist**: Yes / No
- **Notes**: <concerns>
```

### Summary Table
```
| Issue | Title | Status | Fix Present | Tests |
|-------|-------|--------|-------------|-------|
```

For any **FAIL** status, suggest: `/audit-publish docs/audits/AUDIT_REGRESSION_<TODAY>.md`
