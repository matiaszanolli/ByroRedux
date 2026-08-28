# OBL-2026-08-27-01

Issue: #3516 — https://github.com/matiaszanolli/ByroRedux/issues/3516
Filed: 2026-08-27 by /audit-publish from docs/audits/AUDIT_OBLIVION_2026-08-27.md

Source: `docs/audits/AUDIT_OBLIVION_2026-08-27.md` — finding `OBL-2026-08-27-01`

- **Severity**: HIGH
- **Dimension**: 4 — Rendering Path (legacy `NiProperty` chain) / NIFAL material boundary
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:272-276` (consumer) · `crates/nif/src/blocks/properties.rs:417-418` and `:462-464` (the two producers)

## Description

`TexDesc` has two *disjoint* on-disk layouts, and the parser stores them in the **same** `flags: u16` field with **different meanings**:

- `properties.rs` (v < 20.1.0.3, i.e. Oblivion) reads nif.xml's separate `Clamp Mode` / `Filter Mode` / `UV Set` `uint`s and *synthesizes* a packed word with clamp in **bits 0-3**:
  ```rust
  let flags = ((clamp_mode & 0xF) as u16)
      | (((filter_mode & 0xF) as u16) << 4)
      | (((uv_set & 0xF) as u16) << 8);
  ```
- `properties.rs` (v >= 20.1.0.3, i.e. FO3 / FNV / Skyrim) stores the **raw on-disk** `Flags` word, where nif.xml states "clamp and filter mode stored in **upper byte** with `0xYZ00` = clamp mode Y, filter mode Z" — clamp lives in **bits 12-15**.

The single consumer applies the low-nibble decode unconditionally:

```rust
// legacy_properties.rs:272-276
if info.texture_clamp_mode == 3 {
    if let Some(base) = tex_prop.base_texture.as_ref() {
        info.texture_clamp_mode = (base.flags & 0xF) as u8;
    }
}
```

On Oblivion that is right. On FO3/FNV it silently returns 0 — `CLAMP_S_CLAMP_T` — for essentially every legacy-chain material. The in-code comment above that block records the belief that drove this ("the NiTexturingProperty path mirrored the per-slot `flags & 0xF` (#761)"), which is exactly the premise the measurement below falsifies.

## Evidence

Census over both vanilla archives with a throwaway `NiTexturingProperty.base_texture.flags` histogram probe (since removed):

```
=== FNV  (Fallout - Meshes.bsa) ===
base TexDescs: 2258
raw flags histogram: {512: 21, 8704: 1, 12800: 2236}
flags & 0xF      : {0: 2258}          <-- what the code reads
(flags>>12)&0xF  : {0: 21, 2: 1, 3: 2236}   <-- the real clamp mode

=== OBLIVION (Oblivion - Meshes.bsa) ===
base TexDescs: 30120
raw flags histogram: {3: 17, 19: 43, 32: 111, 34: 1, 35: 29948}
flags & 0xF      : {0: 111, 2: 1, 3: 30008}  <-- correct (synthesized layout)
(flags>>12)&0xF  : {0: 30120}
```

`12800 = 0x3200` → clamp 3 (`WRAP_S_WRAP_T`), filter 2. 2236 of 2258 FNV base descriptors author WRAP/WRAP and every one of them resolves to `texture_clamp_mode = 0`. Reachability is not theoretical: the checked-in per-block baselines record `NiTexturingProperty 2077` for Fallout 3 and `3018` for Fallout NV (0 for Skyrim SE / FO4 / FO76 / Starfield).

The value is consumed as a real sampler selection: `crates/renderer/src/texture_registry.rs:171-183` indexes `samplers: [vk::Sampler; 4]` directly by this code, with `0 = CLAMP_S_CLAMP_T` and `3 = WRAP_S_WRAP_T`.

## Impact

Every FO3/FNV mesh that binds its diffuse through a `NiTexturingProperty` chain samples with `VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE` on both axes instead of `REPEAT`. Any UV outside `[0,1]` — i.e. all tiled architecture, terrain trim and repeated detail — smears the border texel across the surface. It fails silently and identically on every draw, so it reads as "the texture looks wrong", not as an error. Oblivion, Skyrim SE, FO4, FO76 and Starfield are unaffected (Oblivion because the synthesized layout genuinely puts clamp in the low nibble; the rest because they carry no `NiTexturingProperty`).

Severity is HIGH per the "wrong/divergent `Material` out of the canonical boundary" rule — one producer, whole-game blast radius, no per-draw fallback to mask it.

## Related

- `#2565` (OBL-D1-04) covers *reader-side* `TexDesc` version gaps and the PS2 L/K divergence — a different defect in the same function; it does not touch the flags-semantics mismatch.
- `#761` is the commit whose reasoning the comment at `legacy_properties.rs:394-396` preserves.
- Sibling of `OBL-2026-08-27-02` (same four lines) — fixing both together is one edit.

## Suggested Fix

Stop overloading one `u16` with two encodings. Either (a) give `TexDesc` an explicit `clamp_mode: u8` decoded at *parse* time in both branches (low nibble in the synth branch, `(flags >> 12) & 0xF` in the raw branch) and have `legacy_properties.rs` read that, or (b) normalise the raw >= 20.1.0.3 word into the same synthesized bit layout at the raw-branch construction site so exactly one encoding ever leaves the parser. (a) is preferable — it removes the ambiguity rather than hiding it. Pin it with a two-case unit test built from the measured real values (`0x3200` → 3, `0x0023` → 3).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
