# Safety Audit — §8 R1 Material Table Invariants

- **Date**: 2026-05-24
- **Scope**: §8 only (R1 material-table invariants, post-`#1248/#1249/#1250/#1251`)
- **Baseline**: `cargo test --workspace` → 0 FAILED
- **Commits in scope**: 454b7a26 (#1248), 005eba25 (#1249), c0374d00 (#1250), c09d63a6 (#1251)

## Result: **No findings.**

All eight R1 invariants pass verification under the new 300 B `GpuMaterial`
layout. The 4 newly added scalar fields (`ior` + `subsurface` + `sheen` +
`sheen_tint` + `anisotropic` — 5 fields totalling 20 B) are correctly
threaded through every contract.

## Per-invariant verification

### 1. Byte-equal hash contract — PASS

- `crates/renderer/src/vulkan/material.rs:737-844` (`hash_gpu_material_fields`) and
  `crates/renderer/src/vulkan/context/mod.rs:471-588` (`DrawCommand::material_hash`)
  walk the same field sequence; the new 5 fields appear in identical order
  at the tail of both walks:
  - `material.rs:835` → `ior`
  - `material.rs:838-840` → `subsurface`, `sheen`, `sheen_tint`
  - `material.rs:842` → `anisotropic`
  - `context/mod.rs:579` → `ior`
  - `context/mod.rs:582-584` → `subsurface`, `sheen`, `sheen_tint`
  - `context/mod.rs:586` → `anisotropic`
- Test fixture `fully_populated_draw_command` at
  `crates/renderer/src/vulkan/context/mod.rs:2594-2602` sets distinct
  non-default values: `ior=1.45`, `subsurface=0.42`, `sheen=0.18`,
  `sheen_tint=0.66`, `anisotropic=0.27` — confirms a drift in either walk
  would surface as a hash mismatch.
- Test `material_hash_matches_gpu_material_field_hash` passes.

### 2. Size pin — PASS

- `crates/renderer/src/vulkan/material.rs:1090-1092`:
  `assert_eq!(std::mem::size_of::<GpuMaterial>(), 300);`
- Manual byte math: 280 (post-#1147) + ior 4 + subsurface 4 + sheen 4 +
  sheen_tint 4 + anisotropic 4 = **300**. All fields are scalar
  `f32`/`u32` with alignment 4 (`gpu_material_alignment_is_4_bytes`
  passes at line 1097-1102); total `300 = 4 × 75` is a multiple of 4, so
  no implicit trailing padding.
- Test name `gpu_material_size_is_260_bytes` is intentionally kept as the
  legacy moniker (rationale at lines 1084-1088 — "function and test name
  kept as `260` so a future size shift updates them in lockstep with the
  assertion"). This is a documented convention, not a bug.

### 3. Offset pin — PASS

`crates/renderer/src/vulkan/material.rs:1304-1313`:
```
ior            → 280  (line 1305)
subsurface     → 284  (line 1308)
sheen          → 288  (line 1309)
sheen_tint     → 292  (line 1310)
anisotropic    → 296  (line 1313)
```
All 5 `offset_of!` assertions present. Test passes.

### 4. GLSL field-name needle pin — PASS

`crates/renderer/src/vulkan/material.rs:1153-1158` adds 5 needles:
`"ior;"`, `"subsurface;"`, `"sheen;"`, `"sheenTint;"`, `"anisotropic;"`.

Confirmed present in `crates/renderer/shaders/triangle.frag`:
- `ior;` → line 147
- `subsurface;` → line 154
- `sheen;` → line 155
- `sheenTint;` → line 156
- `anisotropic;` → line 162

Test `gpu_material_glsl_field_names_pinned` passes.

### 5. Default-equivalence — PASS

**5a. `ior = 1.5 → F0 = 0.04`**:
`crates/renderer/shaders/triangle.frag:672-675`:
```
float dielectricF0FromIor(float eta) {
    float r = (1.0 - eta) / (1.0 + eta);
    return r * r;
}
```
For η=1.5: `r = -0.5/2.5 = -0.2`, `r² = 0.04`. Matches the pre-#1248
hardcoded `vec3(0.04)` byte-for-byte. The consumer at line 1620 reads
`f0Dielectric = dielectricF0FromIor(mat.ior)` unconditionally; the
BGSM_PBR / non-PBR branches at lines 1622-1626 both feed it through
the same `mix()`, so legacy content paths trivially preserve F0.

**5b. Disney lobe gated on `MAT_FLAG_BGSM_PBR`**:
Both direct-light BRDF sites have the gate:
- `triangle.frag:2411-2419` (fallback directional path) → Lambert
  fallback `kD * albedo / PI` when flag clear.
- `triangle.frag:2626-2634` (clustered RIS path) → Lambert fallback
  `kD * albedo` when flag clear (PI scaling matched per the docstring
  at lines 2618-2624).

With defaults `subsurface = sheen = sheen_tint = 0` AND
`MAT_FLAG_BGSM_PBR` unset (every legacy NIF), `disneyDiffuseTerm` is
never invoked — no Lambert-to-Disney swap for legacy content.

**5c. `anisotropic = 0` reduces to isotropic GGX**:
`deriveAxAy` (lines 643-648) with `anisotropic = 0` yields
`aspect = sqrt(1) = 1`, so `ax = ay = alpha = roughness²`. The
algebra:

For a unit half-vector H, `HdotX² + HdotY² + NdotH² = 1`, hence
`HdotX² + HdotY² = 1 - NdotH²`. Substituting `ax = ay = α` into
`distributionGGXAniso`:
```
denom = (HdotX² + HdotY²) / α² + NdotH²
      = (1 - NdotH²) / α² + NdotH²
α² · denom = (1 - NdotH²) + NdotH² · α²
           = 1 + NdotH² (α² - 1)            ← this is denom_iso
D_aniso = 1 / (π · α² · denom²)
        = α² / (π · (α² · denom)²)
        = α² / (π · denom_iso²)             ← exactly D_iso
```

The helper docstring's claim is correct. Additionally, both call sites
gate the anisotropic path on `mat.anisotropic > 0.0` (lines 2386, 2595),
so legacy content with `anisotropic = 0` continues on the cheaper
`distributionGGX` fast path without invoking the helper at all — a
defence in depth even against the `0.025²` floor in `deriveAxAy` that
could otherwise produce a sub-pixel divergence at `roughness < 0.025`
(the gate makes this irrelevant for legacy content).

### 6. Construction site uniformity — PASS

Grep across the workspace finds 7 `DrawCommand {` struct-literal sites.
**Zero** use `..Default::default()`. All 7 explicitly set the 4 new
fields (`ior`, `subsurface`, `sheen`, `sheen_tint`, `anisotropic`):

| File | Line | New-field block |
| --- | --- | --- |
| `byroredux/src/render/static_meshes.rs` | 515 | 560-572 |
| `byroredux/src/render/particles.rs` | 84 | 120-127 |
| `byroredux/src/render/draw_sort_key_tests.rs` | 7 | 38-42 |
| `crates/renderer/src/vulkan/water.rs` | 508 | 535-539 |
| `crates/renderer/src/vulkan/context/draw.rs` | 2937 | 2964-2968 |
| `crates/renderer/src/vulkan/context/mod.rs` | 2561 | 2594-2602 |
| `crates/renderer/src/vulkan/acceleration/tests.rs` | 16 | 43-47 |

Rust's struct-init exhaustiveness check guarantees future fields will
fail to compile without explicit values at these sites (no
`..Default::default()` escape hatches present).

### 7. `to_gpu_material` field forwarding — PASS

`crates/renderer/src/vulkan/context/mod.rs:443-451`:
```rust
// #1248 — per-material refractive index.
ior: self.ior,
// #1249 — Disney diffuse lobe.
subsurface: self.subsurface,
sheen: self.sheen,
sheen_tint: self.sheen_tint,
// #1250 — anisotropic GGX strength.
anisotropic: self.anisotropic,
```

All 5 fields use the same name on both sides — no typo risk where
Rust's struct-literal field syntax would silently accept a mismatch.

### 8. Preset table consistency (#1251) — PASS

`crates/renderer/src/vulkan/material.rs:524-718` adds 6 preset functions
under `pub mod presets`:
- `polished_metal()` (lines 532-545)
- `glass()` (lines 553-563) — only preset that overrides `ior` (1.45)
- `car_paint(base)` (lines 574-583)
- `lacquered_plastic(base)` (lines 589-598)
- `painted_matte(base)` (lines 605-614)
- `skin_wax_marble(base)` (lines 621-634) — overrides `subsurface = 1.0`
  AND sets `BGSM_PBR` flag so the Disney path actually consumes it
  (the test at line 694-699 pins the gate-fires invariant)

7 preset tests at `material.rs:640-717` cover all 6 presets plus the
critical `presets_inherit_defaults_for_unset_fields` guard at lines
706-717, which pins:
- `m.ior == 1.5` on `polished_metal` (default unchanged)
- `m.anisotropic == 0.0` on `polished_metal`
- `m.sheen == 0.0` on `polished_metal`

A future GpuMaterial growth that drifts any preset's inherited default
away from the Hyperion-table values would fail this guard. All 7 tests
pass.

## Methodology notes

- Cross-referenced `crates/renderer/src/vulkan/material.rs`,
  `crates/renderer/src/vulkan/context/mod.rs`, and
  `crates/renderer/shaders/triangle.frag` byte-by-byte for the new
  field group.
- Verified the anisotropic-GGX identity manually via the unit-H
  invariant `HdotX² + HdotY² + NdotH² = 1`.
- Ran targeted tests:
  - `material_hash_matches_gpu_material_field_hash`
  - `gpu_material_size_is_260_bytes`
  - `gpu_material_field_offsets_match_shader_contract`
  - `gpu_material_glsl_field_names_pinned`
  - All 7 `presets::tests::*`
  → 12 passed / 0 failed.
- Full workspace `cargo test` baseline: 0 FAILED.

## Conclusion

R1 invariants are intact after the 280 → 300 byte struct growth.
The lockstep contract between Rust and GLSL holds, both hash walks
include the new fields in identical order, all 7 DrawCommand
construction sites explicitly initialise the new fields (no
`..Default::default()` regression vectors), and the default values
preserve byte-identical legacy behaviour for non-PBR content via:

1. `ior = 1.5 → F0 = 0.04` (matches pre-#1248 hardcoded literal)
2. `subsurface = sheen = sheen_tint = 0` with `MAT_FLAG_BGSM_PBR`
   unset (Disney branch never executes for legacy NIF)
3. `anisotropic = 0` gated to skip the anisotropic NDF call entirely
   (and algebraically reduces to isotropic GGX if it were called)

No findings.
