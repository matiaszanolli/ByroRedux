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

## Resolution log

- The two explicit sites and 4 sibling sites in node.rs/properties.rs landed in **#769** (commit `171d840`) along with boundary tests.
- This issue (#765) closes the parent sweep — 11 additional sites in 7 files:
  - properties.rs:451 (TexDesc PS2 L/K, until=10.4.0.1)
  - base.rs:87 (NiAVObject Velocity, until=4.2.2.0)
  - base.rs:119 (NiAVObject Has Bounding Volume, until=4.2.2.0)
  - tri_shape.rs:125 (MaterialData Has Shader, until=20.1.0.3)
  - tri_shape.rs:1257 (NiTriShapeData Has UV bool, until=4.0.0.2)
  - particle.rs:642 (MaterialData Has Shader, until=20.1.0.3)
  - particle.rs:822 (Rotation Axis particle-info size, until=10.4.0.1)
  - extra_data.rs:73 (NiExtraData parse_legacy gate, until=4.2.2.0)
  - controller/sequence.rs:257 (NiSequence Phase, until=10.4.0.1)
  - texture.rs:54 (NiSourceTexture Use Internal, until=10.0.1.3)
  - texture.rs:667 (NiTextureEffect PS2 L/K, until=10.2.0.0)
- 4 pre-existing boundary tests adjusted from boundary-equality (which had been pinning the pre-fix `<=` behavior) to one-tick-below the boundary.
- Risk: zero on vanilla Bethesda content (no Bethesda NIF lands at any of these boundaries); 100% clean parse rate on FO3/FNV/Skyrim SE preserved.
