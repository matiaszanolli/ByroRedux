# Issues 2617, 2618, 2619, 2620

Four Starfield-audit findings across three domains:
- #2617 → **nif** (`byroredux-nif`) — HIGH — BSEffectShaderProperty stub guard missing
- #2618 → **bsa** (`byroredux-bsa`) — MEDIUM — LZ4 under-run silent truncation
- #2619 → **renderer** (`byroredux-renderer`) — MEDIUM — missing DXGI format arms
- #2620 → **nif** (`byroredux-nif`) — MEDIUM — BSGeometry weights_per_vert==0 stream drift

---

## #2617 — SF-D8-2026-08-07-01: BSEffectShaderProperty stub guard missing — every externally-referenced Starfield effect shader renders invisible

**Severity**: HIGH · **Dimension**: 8 (NIFAL Canonical Material Translation for Starfield)
**Location**: `crates/nif/src/blocks/shader.rs:1616-1650` (`BSEffectShaderProperty::material_reference_stub`), `:1681-1698` (Starfield stub discriminator), `crates/nif/src/import/material/dedicated_shader.rs:365-500` (`apply_bs_effect_shader`, no guard) vs `:85-88` (the `BSLightingShaderProperty` guard that exists), `crates/renderer/shaders/triangle.frag:790-799`
**Status**: NEW — not covered by #2359 (tracks the `.mat`/CDB merge forwarding zero authored data, an approximate-not-invisible outcome) or #2354 (particles)

### Description
`#2353` added `if shader.material_reference { return; }` to the `BSLightingShaderProperty` walker with the rationale that a material-reference stub's fields are parser placeholders, not authored data, and copying them would falsely suppress the external CDB values. `apply_bs_effect_shader` has no equivalent guard — `grep material_reference crates/nif/src/import/` returns exactly one production hit (the BSLSP arm). For a stub, `apply_bs_effect_shader` copies the full placeholder set into `MaterialInfo`: `base_color=[1,1,1,1]` → fabricated emissive tint, `emissive_source` wrongly set to `Effect` (nothing was authored), and — the lethal one — `falloff_start_opacity = falloff_stop_opacity = 0.0`.

### Evidence
```rust
// crates/nif/src/import/material/dedicated_shader.rs:365-... (apply_bs_effect_shader)
// no `if shader.material_reference { return; }` guard anywhere in this function,
// unlike the BSLightingShaderProperty walker at :85-88
```
`triangle.frag:790-799`'s cone-fade math:
```glsl
float coneFade = mat.falloffStartOpacity;
float denom = mat.falloffStartAngle - mat.falloffStopAngle;
if (denom > 1e-5) { ... }
...
finalAlpha = texColor.a * coneFade;
```
The in-shader comment asserts the identity default is `start_op = stop_op = 1.0` ("the math reduces to a no-op"). The stub hardcodes `0.0`, and with `start_angle == stop_angle == 1.0` (also stub defaults), `denom == 0` skips the branch entirely — `coneFade` stays `0.0` → `finalAlpha = 0.0` on every affected surface. Scope: the stub discriminator on Starfield is `!name.is_empty()`, and Starfield FX materials are authored in `materialsbeta.cdb` and referenced by name — i.e. this is the **dominant** path for Starfield effect geometry, not an edge case. Full-body (non-stub) blocks are the ones with an *empty* name.

### Impact
Every externally-referenced Starfield `BSEffectShaderProperty` surface renders fully transparent, with zero visual signal that anything is wrong — a content-visibility failure with no workaround. Per the severity table, "wrong/divergent Material out of NIFAL" is HIGH minimum; this is also flatly worse than divergent — it's invisible.

### Suggested Fix
Mirror the #2353 guard in `apply_bs_effect_shader`: after `info.material_path` capture, `if shader.material_reference { info.material_kind = 101; return; }` (keep the kind tag, drop the placeholder payload). Add a test asserting a stub yields `emissive_source == EmissiveSource::None` and `effect_falloff == None`.

### Related
#2353 (the guard this mirrors, on the sibling type), #2359, #2354.

### Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fix belongs at the NIF-import walker (`apply_bs_effect_shader`), mirroring the existing BSLSP guard — same NIFAL boundary discipline
- [ ] **TESTS**: A stub fixture asserts `emissive_source == EmissiveSource::None` and `effect_falloff == None`

---

## #2618 — SF-D1-01: LZ4 arm silently truncates on under-run; comment claims it hard-errors

**Severity**: MEDIUM · **Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs:738-746` (LZ4 arm), `:712-735` (the misleading comment)
**Status**: NEW — partial overlap with #2097 (LZ4-01, OPEN, LOW), opposite failure direction, different fix

### Description
`lz4_flex::block::decompress(packed, unpacked_size)` allocates the declared size, decodes, then `truncate`s to the actual decoded length and returns `Ok` — so a record that declares *more* than the stream contains gets a silent short buffer, no error, no log. The zlib arm handles the identical condition with `log::warn!` (#812); the comment claiming the LZ4 branch "hard-errors on the same condition" is factually wrong — `lz4_flex` only hard-errors in the *other* direction (declared < actual).

### Evidence
Measured against the pinned `lz4_flex 0.11.6`: under-run → `Ok(len=13)` for a declared 4096, no error; over-run → hard `Err`. Vanilla corpus is clean (0/2,822 sampled chunks), so this is a robustness gap on malformed/mod-repacked archives, not an active bug.

### Impact
LZ4 is the only codec for all 15 Starfield v3 texture archives; a DX10 texture is a concatenation of per-mip chunks, so a short decode on a non-final chunk shifts every subsequent mip, and the synthesized DDS header then misdescribes its own payload — garbled/offset mip data in the renderer with no error signal.

### Suggested Fix
Compare `out.len()` against `unpacked_size` post-decode in the LZ4 arm and `log::warn!` (or hard-error for chunk chains, where a short mid-chain chunk is unrecoverable). Fix the comment. Add an under-run unit test.

### Related
#2097 (LZ4-01), #812, #2360.

### Completeness Checks
- [ ] **TESTS**: An under-run fixture (`unpacked_size` declared larger than the actual decoded stream) asserts a warn/error, not silent truncation

---

## #2619 — SF-D1-03: renderer map_dxgi_format has no arm for DXGI 10/11/31, 78 Starfield textures fall back to placeholder

**Severity**: MEDIUM · **Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/renderer/src/vulkan/dds.rs:508-552` (`map_dxgi_format`)
**Status**: NEW

### Description
The same 78 records SF-D1-02 identifies (missing `pitch_or_linear_size_for` arms for DXGI 10/11/31) also hard-fail the renderer's `map_dxgi_format` — every Starfield interior cubemap and chargen face normal map falls back to the placeholder texture. BA2 extraction of these 78 textures is byte-exact correct; the renderer's DXGI table simply has no arm for 10/11/31 and bails at parse time.

### Evidence
The same 78-record set as SF-D1-02 — 12 interior ambient/reflection-probe cubemaps (`cell_cavecube`, `cell_shipinteriorcube`, …) + the LTC LUT + 62 chargen head normal maps + 2 gas-giant gradients.

### Impact
Missing textures, not a crash — but per the project's own "chrome/posterized ⇒ missing textures" diagnosis rule (see `[[feedback_chrome_means_missing_textures]]`), this is exactly the defect class that costs hours downstream, concentrated on interior ambient lighting and every chargen face.

### Suggested Fix
Add core-Vulkan-1.0 format arms for DXGI 10/11/31 with matching tests.

### Related
SF-D1-02 (BA2 side of the same 78 records).

### Completeness Checks
- [ ] **SIBLING**: Fix alongside SF-D1-02 — same 78-record set, two independent gaps
- [ ] **TESTS**: A fixture for each of DXGI 10/11/31 asserts a valid Vulkan format is returned

---

## #2620 — SF2D2-D2-02: weights_per_vert==0 with nonzero n_total_weights reads zero bytes, drifts rest of .mesh parse

**Severity**: MEDIUM · **Dimension**: 2 (BSGeometry Mesh Extraction)
**Location**: `crates/nif/src/blocks/bs_geometry.rs:479-495`
**Status**: NEW

### Description
`n_total_weights.checked_div(weights_per_vert)` returns `None` only for `weights_per_vert == 0`, and that arm reads **zero bytes** regardless of `n_total_weights`. If a `.mesh` body ever ships `weights_per_vert == 0` with `n_total_weights > 0`, the undrained `BoneWeight` payload shifts every subsequent field (`n_lods`/`n_meshlets`/`n_cull_data`) into garbage, driving `read_u16_triple_array` off a corruption-controlled count.

### Evidence
`crates/nif/src/blocks/bs_geometry.rs:479-495` — the `weights_per_vert == 0` arm reads nothing instead of skipping the payload while advancing the cursor.

### Impact
Parse-position drift on malformed/atypical `.mesh` bodies (per the severity table, MEDIUM for "stream position off"). Bounded by `check_alloc` (no OOM/UB), but the mesh silently loses its LOD/meshlet/cull tables or fails, surfacing only as "REFR spawned with zero meshes" with no diagnostic — Stage B's error arm is `log::debug!`-only.

### Suggested Fix
Treat `weights_per_vert == 0` as "skip the payload, still advance the cursor" (`stream.skip(n_total_weights * 4)`), not "read nothing." Add a unit test with `weights_per_vert = 0`, `n_total_weights = 2`, a non-zero `n_lods` following.

### Related
The deliberate remainder case (`n_total_weights % weights_per_vert != 0`) is correctly pinned by `skin_weights_bulk_read_matches_per_element_semantics`; that test does not cover the `== 0` arm.

### Completeness Checks
- [ ] **TESTS**: A `weights_per_vert=0, n_total_weights=2` fixture with a non-zero trailing `n_lods` pins the cursor-skip fix
