# #765 — NIF-D3-11: `until="N.M.O.P"` boundary uses `<=` not `<`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/765
- **Severity**: LOW
- **Labels**: bug, low, nif-parser, legacy-compat
- **Source**: docs/audits/AUDIT_NIF_2026-04-28.md (NIF-D3-11)

## Summary

niftools' canonical `until=` semantics is exclusive (`version < until_version`). Two Rust gates use `version <= until_version`, over-reading 4 bytes at the exact boundary version:

- [crates/nif/src/blocks/interpolator.rs:294](crates/nif/src/blocks/interpolator.rs#L294) — `NiTransformData::Order` (`until="10.1.0.0"`)
- [crates/nif/src/blocks/properties.rs:233](crates/nif/src/blocks/properties.rs#L233) — `NiTexturingProperty::Apply Mode` (`until="20.1.0.1"`)

No vanilla content lands at either boundary (Bethesda jumps over both 10.1.0.0 and 20.1.0.1), so impact today is zero. Pre-emptive correctness for hand-authored / modder NIFs.

## Fix

Flip both `<=` to `<`. One-line change at each site.

## Test plan

- Boundary-exact regression fixtures (NIF v=10.1.0.0 with NiTransformData XYZ rotation, NIF v=20.1.0.1 with NiTexturingProperty).
- Grep sweep for other `version() <= NifVersion(0x` patterns mapped to `until=` fields.
