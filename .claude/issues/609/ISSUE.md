# D6-NEW-01: MaterialInfo texture paths re-allocated as String (bundle with #231)

## Finding: D6-NEW-01

- **Severity**: LOW
- **Dimension**: String Interning
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`
- **Game Affected**: All
- **Location**: [crates/nif/src/import/material.rs:126-154](crates/nif/src/import/material.rs#L126-L154), [byroredux/src/scene.rs](byroredux/src/scene.rs), [byroredux/src/render.rs](byroredux/src/render.rs)

## Description

#231 covers the NIF header `Arc<str>` table being re-interned into ECS `StringPool` at clip load. A second site has the same shape: `MaterialInfo` carries `Option<String>` and `Vec<String>` for texture paths. The source filename strings are already interned in the NIF header table; each material extraction re-allocates them as fresh heap Strings, then each material clone duplicates them again, and `render.rs` derefs the String each frame for path-based texture resolution.

The `TextureRegistry` already caches by path — the duplication is on the import-staging side, not the GPU side.

## Impact

Per-cell allocator pressure on cell load. Typical interior cell with ~200 meshes × 4 texture slots × ~60-byte path = ~50 KB redundant heap. Not a frame-time issue; a memory-cleanliness and import-throughput finding only.

## Suggested Fix

Bundle with #231 — accept a `&StringPool` parameter into `MaterialInfo::extract` and store `FixedString` instead of `String`. Update `TextureRegistry::resolve` to accept `FixedString` lookups (cheap pointer compare for same-pool symbols).

## Related

- #231 (open): NIF string-table double-interning + clip name heap String. Same root cause; bundle.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Bundle with #231; same StringPool-threading work.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Allocation-count regression on cell-load benchmark (heap-alloc counter, not wall-time).

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`._
