# Issue #233 — NIF-04-11-L1

**Title**: NiParticleSystem uses variant().has_shader_alpha_refs() instead of raw bsver()
**Severity**: LOW
**Dimension**: Block Parsing
**Audit**: `docs/audits/AUDIT_NIF_2026-04-11.md`
**Labels**: low, nif-parser, bug

## Location
`crates/nif/src/blocks/particle.rs:553-557`

## Summary
`particle.rs` gates the Skyrim+ shader/alpha refs via `stream.variant().has_shader_alpha_refs()`, but `base.rs:73` and `node.rs:107` use a direct `stream.bsver() > 34` comparison for the same format condition. Non-Bethesda Gamebryo content above BSVER 34 would silently misalign the stream by 8 bytes.

## Fix
One-line change: replace the variant check with `if stream.bsver() > 34 { ... }`.

## Completeness checks
- [ ] SIBLING scan for other `variant().has_shader_alpha_refs()` callers
- [ ] TEST with synthetic non-Bethesda BSVER>34 header

## Fix with
`/fix-issue 233`
