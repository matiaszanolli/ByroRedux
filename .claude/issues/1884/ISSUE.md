**Severity**: LOW · **Dimension**: Collision/Shader Parsing · **Source**: `docs/audits/AUDIT_NIF_2026-07-05.md` (NIF-D5-001)
**Game Affected**: pre-mainline Oblivion dev content (bsver < 9); all others unaffected
**Status**: NEW (documentation only — NOT a parse divergence)
**Location**: `crates/nif/src/version.rs` (`bsver::RIGID_BODY_EXTRA_FLOATS`)

## Description
The const's docstring states `bhkRigidBody` carries two trailing `Unknown Float 1/2` fields on content with `bsver < RIGID_BODY_EXTRA_FLOATS`. The constant's only consumer is `BhkCollisionObject::parse` (`crates/nif/src/blocks/collision/collision_object.rs`), where it gates the two extra floats on the **bhkBlendCollisionObject** path (`is_blend`), citing nif.xml 3428-3429 (`Unknown Float 1/2` on bhkBlendCollisionObject, `#BSVER# #LT# 9`). The bytes read are correct and match nif.xml; only the const's prose names the wrong owning block.

## Evidence
`collision_object.rs` reads the floats inside `if is_blend { … if stream.bsver() < RIGID_BODY_EXTRA_FLOATS { read f32 ×2 } }`, whereas the `version.rs` docstring attributes them to `bhkRigidBody`.

## Impact
None at runtime. Risk is a future maintainer searching for the field in the rigid-body path and reintroducing a duplicate/mis-gated read.

## Suggested Fix
Reword the docstring to reference `bhkBlendCollisionObject` `Unknown Float 1/2` (nif.xml 3428-3429). No code change. (Optionally rename the constant to `BLEND_COLLISION_EXTRA_FLOATS` if a rename is worth the churn.)

## Related
#549 (the fix that added this gate, correctly, in collision_object.rs).

## Completeness Checks
- [ ] **TESTS**: Doc-only; no regression test needed — verify `cargo test -p byroredux-nif` stays green
