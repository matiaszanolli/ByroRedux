# TD2-2026-10-15-07: `crates/nif/src/anim/types.rs` is a field-for-field twin of `core::animation::types`, bridged by a 200-line copy that has dropped fields before

Labels: medium,animation,nif-parser,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-07`).

- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/nif/src/anim/types.rs:47-224` (`TransformChannel`, `TranslationKey`, `RotationKey`, `ScaleKey`, `AnimFloatKey`, `AnimColorKey`, `AnimBoolKey`, `FloatChannel`, `ColorChannel`, `BoolChannel`, `TextureFlipChannel`, `AnimationClip`) vs `crates/core/src/animation/types.rs:50-235`; bridge `byroredux/src/anim_convert.rs:556-760` (`convert_nif_clip`)
- **Status**: NEW (#3862 unified only `FloatTarget`/`ColorTarget`; its own comment says "the canonical vocabulary for both sides of the NIF boundary")
- **Age**: `8b72dbdcb` (#3097, 2026-08-18) added `phase` NIF-side; `d1bcf6e2b` (#3345, 2026-08-28) added it core-side; `f680df2e0` (#4167, 2026-09-12) added completeness guards for the twin
- **Effort**: medium
- **Description**: `byroredux-nif` already depends on `byroredux-core`, yet it declares its own `[f32; 3]`/`[f32; 4]` keyed structs and `convert_nif_clip` maps each key vector element-by-element into the glam ones. The eight key/channel structs carry no information the core ones lack (the two real translations - `TexType` slot -> `FlipTextureRole`, `String` text keys -> `FixedString` - are the only reason the clip struct cannot be shared as-is).
- **Evidence**: the bridge's own comment records the failure mode: "#3345 - `NiControllerSequence.phase` reached the NIF-side clip under #3097 but stopped here: this literal copied every sibling field and not this one, and `byroredux_core`'s `AnimationClip` had nowhere to put it"; #3258 and #3432 then added `sanitized_clip_frequency/duration/weight` at the same seam because the raw fields flowed through unvalidated.
- **Impact**: any clip field added NIF-side (the Starfield/FO4 animation work in flight) is dropped until someone notices, and #4167's completeness guards pin the symptom rather than remove the second declaration.
- **Related**: #3862, #3345, #3258, #3432, #4167, NIFAL (this is a legitimate Imported -> Canonical boundary for the *clip*, not for the key structs).
- **Suggested Fix**: have `nif::anim` emit `byroredux_core::animation::{TranslationKey, RotationKey, ScaleKey, AnimFloatKey, AnimColorKey, AnimBoolKey, FloatChannel, ColorChannel, BoolChannel, TransformChannel}` directly (delete the nif twins, the `convert_nif_clip` key loops shrink to the interning and texture-flip arms), and keep only `AnimationClip`/`TextureFlipChannel` as the Imported tier.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
