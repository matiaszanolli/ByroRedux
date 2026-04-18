# NIF-D3-C1: NiTexturingProperty over-consumes 1 byte on Oblivion (has_normal/has_parallax reads not version-gated)

**Issue**: #429 — https://github.com/matiaszanolli/ByroRedux/issues/429
**Labels**: bug, nif-parser, critical, legacy-compat

---

## Finding

`crates/nif/src/blocks/properties.rs:259-272` reads `normal_texture` when `texture_count > 6` and `parallax_texture` when `texture_count > 7` **unconditionally** on the NiTexturingProperty parse path:

```rust
let normal_texture = if texture_count > 6 {
    Self::read_tex_desc(stream)?       // <-- no version gate
} else {
    None
};

if texture_count > 7 {
    let parallax = Self::read_tex_desc(stream)?;   // <-- no version gate
    if parallax.is_some() {
        let _parallax_offset = stream.read_f32_le()?;
    }
}
```

Per nif.xml (lines 5229-5270), `Has Normal Texture` is `since="20.2.0.5"` and `Has Parallax Texture` is `since="20.2.0.5"` — **both absent at Oblivion v20.0.0.5**. The decal loop below (line 276) IS version-gated, but the normal/parallax reads are not. Asymmetric.

## Impact

Oblivion NIFs with `texture_count > 7` over-consume bytes:
- `texture_count == 7`: phantom `has_normal` bool → over-consume 1 byte + potential TexDesc body if `has == 1`.
- `texture_count == 8`: phantom `has_normal` + phantom `has_parallax` → over-consume 2 bytes + potential bodies + parallax_offset f32.

Oblivion has **no block_sizes table**. Every subsequent block misaligns. Eventually a downstream block reads junk and either returns `Err` (recovered as NiUnknown) or fabricates a huge count (OOM class from #388).

Explains decal-heavy Oblivion clutter drift not covered by #388 / #395. Inverse of the historical "1-byte shortfall" symptom.

## Evidence — nif.xml slot layout

Pre-v20.2.0.5 (Oblivion's v20.0.0.5):
- Slots 0-5: base / dark / detail / gloss / glow / bump (6 "has" bools, already read)
- Slots 6+: **decal 0, decal 1, decal 2, decal 3** (`Texture Count #GT# 6`, `>7`, `>8`, `>9`)
- **No normal slot, no parallax slot.**

v20.2.0.5+:
- Slots 0-7: base / dark / detail / gloss / glow / bump / **normal** / **parallax**
- Slots 8+: decals

## Fix

Gate the two reads on version:

```rust
let normal_texture = if stream.version() >= crate::version::NifVersion(0x14020005)
    && texture_count > 6
{
    Self::read_tex_desc(stream)?
} else {
    None
};

if stream.version() >= crate::version::NifVersion(0x14020005) && texture_count > 7 {
    let parallax = Self::read_tex_desc(stream)?;
    if parallax.is_some() {
        let _parallax_offset = stream.read_f32_le()?;
    }
}
```

The existing decal loop at line 276 already picks the right `num_decals` formula per version branch — no change needed there once the normal/parallax reads are gated out of Oblivion's path.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify no other "has_X_texture" bool reads outside the version-gated block; scan lines 240-290 for unconditional `read_tex_desc` on v20.0.0.5.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NiTexturingProperty with version=20.0.0.5, texture_count=8, no trailing shader textures. Assert parse consumes exactly `base_bool + 6 slot bodies + 2 decal_has_bools + 2 decal TexDesc bodies`, NOT `+ normal_has + parallax_has + parallax_offset`. Add Oblivion-specific fixture to `crates/nif/tests/` that exercises a decal-heavy clutter mesh from the vanilla Oblivion BSA.

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 3 C1.
