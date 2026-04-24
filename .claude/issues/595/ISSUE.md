# #595: FO4-DIM2-04: Stale cubemap comment — '0x0800 = cubemap?' (actual bit is 0x1)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/595
**Labels**: documentation, import-pipeline, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: LOW
**Location**: `crates/bsa/src/ba2.rs:345`
**History**: Carry-forward of AUDIT_FO4_2026-04-17 L2, never previously filed.

## Description

Source comment is stale. Code correctly detects cubemap via `flags & 0x1` at line 354, but the comment two lines above says `// base[22..24] flags (0x0800 = cubemap?)`. `0x0800` is a different bit (community-reverse-engineered as "tile mode"); `0x0001` is the cubemap indicator confirmed against vanilla FO4 archives (Textures1.ba2 cubemap test pattern) in the prior audit.

## Evidence

```rust
// ba2.rs:345
// base[22..24] flags (0x0800 = cubemap?)
// ...
// ba2.rs:353-354
// Bit 0 of the flags is the "is cubemap" indicator in FO4 DX10 archives.
let is_cubemap = flags & 0x1 != 0;
```

## Impact

Docs-only. Trap for the next contributor debugging cubemap routing.

## Suggested Fix

Delete the misleading `0x0800` comment; keep the accurate one at line 353.

## Completeness Checks

- [ ] **TESTS**: n/a (doc-only change)
