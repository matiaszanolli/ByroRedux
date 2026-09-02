# #3452 — REN-2026-08-27-D6-01: FO4 Rimlight Power FLT_MAX sentinel carried into GpuMaterial

**Severity**: MEDIUM · **Location**: `crates/nif/src/blocks/shader.rs`:1070-1119 (`parse_fo4`), `crates/nif/src/import/material/dedicated_shader.rs:336`, `byroredux/src/material_translate.rs:520`
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-27.md` (REN-2026-08-27-D6-01)

`parse_fo4` correctly reads `Backlight Power` iff `Rimlight Power >= FLT_MAX` (nif.xml's discriminator convention), but stores the raw `FLT_MAX` sentinel verbatim into `rimlight_power` instead of normalizing it. It survives NIFAL untouched (finite, so `fix_scalar!` doesn't catch it) and reaches `bethesdaRimFactor`, where `clamp(FLT_MAX, 0.25, 16.0)` = 16.0 — the tightest possible rim, for a material that authored none.

**Suggested Fix**: Normalize at the parser: when the `FLT_MAX` branch is taken, store `rimlight_power` as a real no-value default (BGSM's own `2.0`, or `0.0`) instead of the sentinel.

---

# #3478 — NIF-2026-08-27-D6-01: hkSubPartData table uses 1-byte allocation bound for a 12-byte element

**Severity**: LOW · **Location**: `crates/nif/src/blocks/collision/shape_mesh.rs:215`
**Source**: `docs/audits/AUDIT_NIF_2026-08-27.md` (Dimension 6)

`sub_parts = stream.allocate_vec(num_sub_shapes as u32)?;` uses the 1-byte-per-element bound, but `HkSubPartData` is 3×`u32` = 12 bytes. Should use `allocate_vec_sized::<HkSubPartData>`.

**Suggested Fix**: `stream.allocate_vec_sized::<HkSubPartData>(num_sub_shapes as u32)?`.

---

# #3479 — NIF-2026-08-27-D4-01: SSE triangle-drop diagnostic still names vertex_map

**Severity**: LOW · **Location**: `crates/nif/src/import/mesh/sse_recon.rs:159-164`
**Source**: `docs/audits/AUDIT_NIF_2026-08-27.md` (Dimension 4)

#3355 retargeted the drop bound from `vertex_map` to `decoded.positions.len()`, but the log message at line 162 still says "out-of-range vertex_map indices". Diagnostic-only, no `vertex_map` read remains on this path.

**Suggested Fix**: Reword to "out-of-range global vertex indices (past the decoded buffer's vertex count)".

---

# #3507 — FO4-2026-08-27-D5-01: BSLightingShaderProperty.texture_clamp_mode parsed then dropped

**Severity**: MEDIUM · **Location**: `crates/nif/src/import/material/dedicated_shader.rs` (`apply_bs_lighting_shader`), field in `crates/nif/src/blocks/shader.rs`
**Source**: `docs/audits/AUDIT_FO4_2026-08-27.md` (FO4-2026-08-27-D5-01)

`apply_bs_lighting_shader` copies `uv_offset`/`uv_scale`/`alpha`/etc. from `BSLightingShaderProperty` but never reads `texture_clamp_mode`, so it keeps the `MaterialInfo` default (3, WRAP_S_WRAP_T) for FO4/Skyrim/FO76/Starfield lit materials. Measured 7.88% of vanilla FO4 lit materials author a non-default mode (architecture wall-kit trim, LOD atlases). The BGSM half (`tile_u`/`tile_v`) is dropped too — `merge_external_material` never maps it onto `texture_clamp_mode`.

**Suggested Fix**: In `apply_bs_lighting_shader`, add (gated on `!info.texture_clamp_mode_consumed`):
```rust
info.texture_clamp_mode = shader.texture_clamp_mode as u8;
info.texture_clamp_mode_consumed = true;
```
Map BGSM `tile_u`/`tile_v` onto the same field in `merge_external_material`.

**Related**: #610 (identical fix for `NiTexturingProperty`/`BSEffectShaderProperty`), #2328 (`_consumed` precedence gate), #2571 (put `texture_clamp_mode` on canonical `Material`).
