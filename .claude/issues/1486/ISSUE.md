## Finding REN2-01 — Renderer Audit 2026-06-11

- **Severity**: HIGH
- **Dimension**: GPU Skinning (independently found by Dims 1, 3, 5, 6, 11, 12)
- **Location**: `crates/renderer/shaders/triangle.vert:142-165,190`; root data path `byroredux/src/render/skinned.rs:162` → `crates/renderer/src/vulkan/context/draw.rs:835-841` → `crates/renderer/shaders/skin_palette.comp:78`; rigid-only rebase at `crates/renderer/src/vulkan/context/draw.rs:1759-1768`
- **Status**: NEW — regression introduced by `36f66493` (PR #1485). Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The camera-relative cascade rebased rigid per-instance model translations and made the uploaded `viewProj` camera-relative, but bone palettes remain absolute world (`bone_world × bind_inverse`, placement included) and the skinned vertex branch ignores `inst.model` entirely. `worldPos = Σwᵢ·palette[bᵢ] · pos` is absolute, fed into the relative `viewProj` → rendered as if at `p + render_origin`. Additionally `fragWorldPos = worldPos.xyz + renderOrigin.xyz` (`triangle.vert:190`) **double-adds** the origin for skinned fragments, so even visible skinned fragments get lighting/RT-origin/fog positions wrong by +origin.

## Evidence (validated at HEAD)

- `triangle.vert:142-155` — skinned branch builds `xform` purely from `bones[base + bIdx.*]`, zero `renderOrigin` references; rigid branch at `:139` uses the CPU-rebased `inst.model`.
- `triangle.vert:190` — `fragWorldPos = worldPos.xyz + renderOrigin.xyz;` applied unconditionally (double-add for skinned).
- `byroredux/src/render/skinned.rs:162` — `Some(gt) => gt.to_matrix()` uploads unmodified absolute bone world matrices.
- `draw.rs:1759-1768` — translation rebase applies only to the per-instance `model` matrix; no equivalent for the bone palette.
- The `#markarth-precision` comment above `fragWorldPos` asserts "`worldPos` is in render-origin-relative space" — false for the skinned branch.

## Impact

Whenever `render_origin ≠ 0` — camera outside the single `[0,4096)³` box, i.e. virtually all exterior play and any interior with a negative camera coordinate (the `floor()` snap makes any negative component yield −4096) — every skinned mesh (NPCs, creatures) rasterizes displaced ≥4096 units, typically invisible. Pre-delta this path was correct. The Markarth verification scene contained no actors, which is why this shipped unnoticed.

## Suggested Fix

In the skinned branch subtract `renderOrigin.xyz` from `worldPos` (and `prevWorldPos`) immediately after the palette blend, making the skinned path origin-relative like the rigid path; `fragWorldPos = worldPos + renderOrigin` then becomes uniformly correct. Keep palettes absolute so `skin_vertices.comp`/BLAS stay world-space. Recompile `triangle.vert.spv` (plain `-V`, not `-g0` — reflection test needs OpName).

## Related

REN2-02 (skinned TLAS double-transform), REN2-04 (prev-frame origin); `docs/smoke-tests/m41-equip.sh` in an exterior cell is the runtime regression check once fixed.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers) — esp. `skin_vertices.comp` consumers and prev-position path
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer
- [ ] **TESTS**: Regression test added for this specific fix

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
