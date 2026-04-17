# OBL-D5-H2: 9 Oblivion block types have no parser and cannot be skipped (no block_sizes)

**Issue**: #394 — https://github.com/matiaszanolli/ByroRedux/issues/394
**Labels**: bug, nif-parser, high, legacy-compat

---

## Finding

Oblivion NIFs have no `block_sizes` table (that's a Fallout 3+ feature). When the block dispatcher at `crates/nif/src/blocks/mod.rs:606-630` hits an unknown type name, the error is terminal — there's no size hint to skip with (except the `oblivion_skip_sizes` static table at `lib.rs:285` which covers only known sizes).

## Block types observed in vanilla Oblivion sweep

| Block type | Files affected | Content impact |
|---|---|---|
| `NiBSBoneLODController` | 34 | Creature skeleton LOD — cascades into full animation loss |
| `NiColorData` | 17 | Per-key RGB animation (breathing glow, candles) |
| `NiFogProperty` | 3 | Ancient fog property (pre-BSShader era) |
| `NiFlipController` | ? | Texture-flip anim (water ripples, caustics) |
| `NiPathInterpolator` | ? | Spline-path interpolator (door hinges) |
| `NiBoolTimelineInterpolator` | ? | Gated anim visibility |
| `bhkMultiSphereShape` | 1 | Havok sphere cluster — creature collision |
| `bhkBlendController` | ? | Rigid-body blend |

## Impact

Most vanilla Oblivion creature NIFs use `NiBSBoneLODController` — when that fails to parse, every block after it is discarded (no size to skip). Creature cells render with skeletal geometry but no animation (when OBL-D6-4 is fixed), or nothing at all (before).

## Fix

Two layers:
1. **Stub parsers with signature + size walk** for the 9 known-used types. Pattern already exists: `BhkConstraint::parse` at `crates/nif/src/blocks/collision.rs:571-591` does this for 7 constraint subtypes.
   - `NiColorData` is trivial (NiKeyframeData clone for RGBA).
   - `NiBSBoneLODController` is a short controller with bone-count + ranges.
   - `NiFogProperty` is 3 fields (start, end, color).
2. **Add remaining 6 to `oblivion_skip_sizes`** at `crates/nif/src/lib.rs:285` — a safety net for rarely-encountered types.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Each new parser must be added to the dispatcher at `blocks/mod.rs:120-630`.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression parse test for one Oblivion creature NIF containing `NiBSBoneLODController` (e.g., `meshes\creatures\imp\imp.nif` if it has one, else pick from a goblin/rat).

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 5 H-2.
