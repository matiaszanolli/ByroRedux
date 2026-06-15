# FO3-1-01: intern_texture_path does not guard whitespace-only paths

- **Issue**: #1541
- **Severity**: LOW
- **Labels**: low, nif-parser, bug
- **Dimension**: Inline Shaders / Material
- **Source audit**: docs/audits/AUDIT_FO3_2026-06-14.md (FO3-1-01)
- **Location**: `crates/nif/src/import/material/mod.rs:33-39`

## Description
`intern_texture_path` collapses a texture-slot string to `None` only when
`path.is_empty()`. A non-empty all-whitespace string (`" "`, `"\t"`) passes the
guard and is interned to `Some(sym)`.

## Evidence
```rust
pub(super) fn intern_texture_path(pool: &mut StringPool, path: &str) -> Option<FixedString> {
    if path.is_empty() { None } else { Some(pool.intern(path)) }
}
```
No `.trim()`. The normal slot is safe (spawn-side `h != fallback()` guard at
`byroredux/src/cell_loader/spawn.rs:883`), but the diffuse slot binds
unconditionally (`spawn.rs:721`/`:846`), so a whitespace-only
`BSShaderNoLightingProperty.file_name` binds the magenta checker placeholder —
the `None`→`neutral_fallback()` early-out at `asset_provider.rs:580-582` is
bypassed because the value is `Some(" ")`, not `None`.

## Impact
Near-zero vanilla incidence; defensive/robustness gap on malformed content.
Cross-game — single chokepoint shared by all games.

## Suggested Fix
`if path.trim().is_empty()` at the chokepoint; unit test asserting
`intern_texture_path(pool, "  ") == None`.

## Completeness Checks
- [ ] SIBLING: all texture-slot interning routes through this one helper
- [ ] CANONICAL-BOUNDARY: fix stays at NIF import→Material boundary
- [ ] TESTS: regression test pins `intern_texture_path(pool, "  ") == None`
