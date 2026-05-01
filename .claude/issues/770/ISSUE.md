# Issue #770: NIF-DIM2-02 — Dead Fallout3 arm in `has_shader_emissive_color`

**Severity**: LOW (cosmetic) · **Domain**: nif-parser · **Type**: bug
**Source audit**: docs/audits/AUDIT_NIF_2026-04-30.md
**Related**: #724 (parent dead-code pattern)

## Summary

`crates/nif/src/version.rs:184-189` includes `Self::Fallout3` in the match arm of `has_shader_emissive_color`. FO3 ships BSVER=21; the field is gated `vercond="#BS_GT_FO3#"` (bsver > 34). FO3 fails this check on disk so the predicate's FO3 arm is unreachable. Functionally inert but misleads readers.

## Fix

Remove `Self::Fallout3` from the match arm. Document that the in-file `bsver > 34` check is the real guard.

## Completeness checks

- [ ] Sweep other `has_*` predicates in `version.rs` for similar dead arms
- [ ] Add comment about variant predicate vs in-file BSVER gate relationship
