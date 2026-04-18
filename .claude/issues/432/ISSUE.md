# NIF-COV-04: BSAnimNote / BSAnimNotes missing — KF animation events silently dropped on import

**Issue**: #432 — https://github.com/matiaszanolli/ByroRedux/issues/432
**Labels**: bug, animation, nif-parser, high

---

## Finding

`BSAnimNote` (nif.xml:6878) and `BSAnimNotes` (nif.xml:6887) — both `versions="#FO3_AND_LATER#"` — are not in the `parse_block` dispatch table at `crates/nif/src/blocks/mod.rs`.

They are attached to `NiControllerSequence` to fire gameplay events at precise animation times. Skyrim exported these heavily.

## Impact

Every `.kf` file for FO3/FNV/Skyrim/FO4 contains per-frame event markers: footsteps, weapon impact frames, spell-cast timing, SFX triggers, hit-frame decisions. Currently those blocks land on `NiUnknown`, so when KF→ECS animation-clip conversion runs in `crates/nif/src/anim.rs` / `byroredux/src/anim_convert.rs`, the event channel is empty.

Downstream consumers:
- `crates/core/src/animation/text_events.rs` — the ECS event channel that would receive these triggers. Already exists and is functional.
- Footstep SFX system — waiting for per-frame event hooks.
- Combat damage-frame logic — waiting for hit-frame events.
- Magic VFX spawn timing — relies on cast-stage events.

## Games affected

FO3, FNV, Skyrim LE/SE, FO4.

## Fix

Add to `crates/nif/src/blocks/extra_data.rs` (both types extend NiExtraData):

```rust
/// BSAnimNote — single animation event marker.
/// versions: #FO3_AND_LATER#
#[derive(Debug)]
pub struct BsAnimNote {
    pub base: NiExtraData,
    pub kind: u32,          // 1 = sound, 2 = grab-ik, 3 = look-ik
    pub time: f32,
    pub text: Option<Arc<str>>,
    pub event_code: u32,
    pub arm_code: u32,
}

/// BSAnimNotes — list of BSAnimNote refs.
#[derive(Debug)]
pub struct BsAnimNotes {
    pub base: NiExtraData,
    pub notes: Vec<BlockRef>,  // refs to BsAnimNote blocks
}
```

Parse both, add dispatch arms, wire into `anim_convert.rs` to populate `AnimationClip.events` during KF import.

~20 LOC parser, ~30 LOC anim_convert wiring.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify no other NiExtraData subclass we already parse references a BSAnimNotes ref; the list semantics are independent but worth cross-checking.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Import a Skyrim `1hmcombattoidle.kf` (has ~5 foot-contact events). Assert `AnimationClip.events.len() >= 5` and each event has the expected time/type.

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 5 COV-04.
