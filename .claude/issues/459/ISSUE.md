# Issue #459

FO3-NIF-L1: BSShaderTextureSet::parse silently clamps negative num_textures to zero

---

## Severity: Low

**Location**: `crates/nif/src/blocks/shader.rs:175`

## Problem

```rust
let num_textures = stream.read_i32_le()?;
let mut textures = stream.allocate_vec(num_textures.max(0) as u32)?;
```

`num_textures` read as `i32` with `max(0) as u32` clamp — a negative count silently becomes an empty set. nif.xml documents `Num Textures` as `uint`; a negative means the stream is off-rails.

## Impact

Masks upstream drift. Unlikely on vanilla FO3 NIFs but contributes to "mysterious missing textures" debugging when a mod NIF is misaligned.

## Fix

Either:
1. Read as `u32` directly: `stream.read_u32_le()?`.
2. Keep `i32` but return `io::Error::other("negative texture count")` when negative, so the outer block-sizes recovery catches it loudly.

Prefer (1) for parity with nif.xml.

## Completeness Checks

- [ ] **TESTS**: Synthetic stream with negative count → parse fails cleanly with diagnostic
- [ ] **SIBLING**: Audit other `read_i32_le()` reads throughout `crates/nif/src/blocks/` — same anti-pattern?

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-NIF-L1)
