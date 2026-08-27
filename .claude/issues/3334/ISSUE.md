# FNV-2026-08-26-D1-04

**Issue**: #3334
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 1 — Cell Loading End-to-End
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/asset_provider/archive.rs:137-168` vs
`byroredux/src/asset_provider/texture.rs:374` and
`crates/renderer/src/texture_registry.rs:1907`

**Premise verified**: `strip_build_prefix` scans for a `\data\`/`/data/` boundary that
requires a separator on *both* sides:

```rust
// archive.rs:154-160
if (l == b'\\' || l == b'/') && (r == b'\\' || r == b'/')
    && bytes[i + 1..i + 5].eq_ignore_ascii_case(b"data")
```

A path that *starts* with `Data\` has no leading separator and is returned unchanged.
`resolve_texture_view_with_clamp` (`texture.rs:374`) uses that result as the
`acquire_by_path` / `enqueue_dds` cache key, and the registry's `normalize_path`
(`texture_registry.rs:1907`) prepends `textures/` to anything not already starting
with it:

```
"Data\Textures\Water\WastelandWaterPotomac.dds"
  → strip_build_prefix  → unchanged
  → normalize_path      → "textures/data/textures/water/wastelandwaterpotomac.dds"
```

Extraction itself succeeds — `TextureProvider::extract` routes through
`normalize_texture_path`, which *does* strip a leading `data\`
(`asset_provider/tests/material_path.rs:235` pins that FaceGen case) — so the texture
loads; only the key is wrong-shaped.

**Evidence**: every FNV `WATR.NNAM` carries the prefix, e.g.
`NVCleanWater → Data\Textures\Water\WastelandWaterPotomac.dds` (dumped from
`FalloutNV.esm`; ~55 of ~60 records share that one path).

**Impact**: Small on its own — one duplicate bindless slot + one duplicate GPU upload
whenever the same physical DDS is also referenced in canonical `textures\…` form.
It matters mainly because it *guarantees* FNV-2026-08-26-D1-02 fires: the WATR
resolve can never hit a cache entry populated by the REFR walk, so it is always a
fresh (unflushed) enqueue. Same key-drift shape as #3038, one layer down.

**Fix sketch**: Let `strip_build_prefix` also accept a `data\`/`data/` segment at
offset 0 (the `normalize_texture_path` rule already does), or make
`resolve_texture_view_with_clamp` key on `normalize_texture_path`'s output so the
cache key and the archive lookup agree by construction.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
