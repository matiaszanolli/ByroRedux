# Issue #768: NIF-D3-13 — bs_geometry weights_per_vert inner-loop allocation bypasses guard

**Severity**: MEDIUM · **Domain**: nif-parser, safety · **Type**: bug
**Source audit**: docs/audits/AUDIT_NIF_2026-04-30.md
**Related**: #764 (parent allocate_vec budget guard)

## Summary

`crates/nif/src/blocks/bs_geometry.rs:450` does `Vec::with_capacity(weights_per_vert as usize)` inside a loop where `weights_per_vert` is file-driven and unbounded. Hostile NIF triggers OOM panic before stream reads can fail. The outer loop is bounded via `allocate_vec`, but the inner allocation slips through.

## Fix

Either route through `allocate_vec` (`stream.allocate_vec::<BoneWeight>(weights_per_vert)?`) OR remove `with_capacity` and let `Vec::push` allocate organically.

## Completeness checks

- [ ] Sweep all `Vec::with_capacity(file_driven_value as usize)` in `crates/nif/src/blocks/`
- [ ] Regression test with hostile `weights_per_vert = 0xFFFFFFFF` payload
- [ ] Confirm matches #764's `allocate_vec` budget contract
