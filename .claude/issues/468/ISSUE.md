# Issue #468

FNV-3-H1: WTHR cloud textures lack textures\ prefix — FNV exterior clouds silently disabled

---

## Severity: High

**Location**: `byroredux/src/scene.rs:166-204`

## Problem

`wthr.cloud_textures[0]` is a textures-root-relative zstring (e.g. `"sky\cloudsnoon.dds"`) per UESP and the parser's own test (`records/weather.rs:280`). `scene.rs:169` hands this directly to `tex_provider.extract(path)`, but `Fallout - Textures.bsa` stores these paths as `textures\sky\cloudsnoon.dds` (verified via `crates/bsa/src/archive.rs:487 normalize_path` + test at `:645`).

Lookup always misses → `cloud_tile_scale = 0.0` → every FNV exterior renders with clouds disabled. The fallback branch at `scene.rs:194,200,203` also hardcodes `0.0`, so the cloud path is end-to-end dead.

## Contrast

`byroredux/src/cell_loader.rs:700` correctly prepends `textures\landscape\` before `resolve_texture`. The behaviour diverged from `resolve_texture` itself.

## Impact

Every FNV exterior cell renders with no clouds. Capital Wasteland / Mojave / Capital DLC all affected. Visually flat sky.

## Fix

In `scene.rs:169`, normalize path before `extract`:

```rust
let path_normalized = if path.to_lowercase().starts_with("textures\\") {
    path.to_string()
} else {
    format!("textures\\{}", path)
};
```

Apply the same normalization to `climate.sun_texture` (currently unused — tracked as FNV-3-L3). Factor into `resolve_texture` so both callers share one normalizer.

## Completeness Checks

- [ ] **TESTS**: Load FNV exterior cell, assert `SkyParamsRes.cloud_tile_scale > 0.0`
- [ ] **SIBLING**: `climate.sun_texture` path (FNV-3-L3) needs the same fix
- [ ] **SIBLING**: Cross-game check — FO3/FO4/Skyrim WTHR cloud paths likely have the same issue
- [ ] **DOCS**: Document the `textures\` prefix convention in `TextureProvider::extract` doc comment

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-3-H1)
