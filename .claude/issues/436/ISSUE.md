# NIF-D1-M3: NiTransformData XYZ_ROTATION_KEY reads only X channel — Skyrim creature animations broken

**Issue**: #436 — https://github.com/matiaszanolli/ByroRedux/issues/436
**Labels**: bug, animation, nif-parser, medium

---

## Finding

`crates/nif/src/blocks/keyframe.rs` — `NiTransformData` / `NiKeyframeData` rotation parser. When `rotation_type == 4` (XYZ_ROTATION_KEY), nif.xml specifies:

1. `num_rotation_keys` == 0 (no quaternion keys in this mode)
2. Three independent `KeyGroup<float>` blocks for X, Y, Z Euler channels

Current implementation reads `num_rotation_keys` and branches on `rotation_type`, but the XYZ path reads only **one** `KeyGroup<float>` before falling through — Y and Z key groups are silently skipped.

## Impact

Three shipped Skyrim creatures use XYZ rotation keys heavily:
- Dragon wing flap animation
- Horse gallop cycle
- Mammoth stomp animation

Affected animations play with only the X-axis channel; Y and Z silently zero. Additionally, the second and third KeyGroup bytes are still in the stream after the parser moves on → subsequent block offsets shift → downstream blocks parse garbage.

Oblivion may also be affected for a narrow set of creature anims that use XYZ Euler (uncommon there — most Oblivion creatures use quaternion keys).

## Games affected

Skyrim LE/SE (primary), Oblivion (narrow).

## Fix

After branching on `rotation_type == 4`, read all three KeyGroups:

```rust
if rotation_type == 4 {
    // XYZ Euler mode — num_rotation_keys == 0, then three KeyGroup<float>:
    let x_keys = parse_key_group_f32(stream)?;
    let y_keys = parse_key_group_f32(stream)?;
    let z_keys = parse_key_group_f32(stream)?;
    // Store as Euler keys for Y-up → quaternion conversion at sample time.
    self.euler_keys = Some(EulerKeyGroups { x: x_keys, y: y_keys, z: z_keys });
}
```

The animation sampler then composes X/Y/Z Euler keys into a quaternion at each sample time (Gamebryo CW-positive convention per memory note — negate angles for glam).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: `NiKeyframeData` shares the rotation_type branch logic with `NiTransformData`. Fix needed in both or factor into a shared helper.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Parse Skyrim's `dragon_wingflap.kf` or `horse_gallop.kf`. Assert the parsed NiTransformData has three non-empty Euler key groups. Sample at t=cycle_duration/4 and assert the resulting quaternion is NOT identity.

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 1 M3.
