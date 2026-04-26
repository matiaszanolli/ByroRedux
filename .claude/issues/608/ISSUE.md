# N2-NEW-01: VF_INSTANCE vertex desc flag declared but no decoder — instance stream silent-dropped

## Finding: N2-NEW-01

- **Severity**: LOW
- **Dimension**: NIF Format Readiness
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`
- **Game Affected**: FO4, FO76, Starfield (instanced statics)
- **Location**: [crates/nif/src/blocks/tri_shape.rs:361-364](crates/nif/src/blocks/tri_shape.rs#L361-L364)

## Description

Closed #358 / #336 added `VF_INSTANCE = 0x200` as a constant alongside `VF_UVS_2` and `VF_LAND_DATA`, with round-trip unit tests. The constant is referenced only inside its own unit tests (lines 2362, 2382). `BsTriShape::parse` walks the desc word and absorbs the bytes via the trailing skip; instance-stream payload is never decoded into a structured field.

## Impact

FO4+ instanced terrain pieces (cliffs, rocks reused thousands of times in worldspaces) carry per-instance data the renderer never receives. Currently the renderer doesn't consume per-vertex instance data anyway, so end-to-end visual impact is zero — flagging because the gap will surface when GPU-driven instancing is wired (related to the instanced-draw work in M31).

## Suggested Fix

When VF_INSTANCE consumption is wired through the renderer:

1. Decode the per-vertex instance attribute slice in `BsTriShape::parse` (size depends on `BsTriShape` semantic; cross-check nif.xml).
2. Forward as `instance_stream: Option<Vec<...>>` on the imported mesh.
3. Add a debug counter ahead of the fix to confirm vanilla content actually exercises the bit (currently unverified).

## Related

- #358 (closed): added the constant.
- #336 (closed): added VF_UVS_2 / VF_LAND_DATA companions.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: VF_UVS_2 and VF_LAND_DATA share the same skip-only treatment — verify they have decoders if/when consumers land.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a vanilla-content audit (counter sweep of FO4 BA2 mesh archives) before adding the decoder.

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`._
