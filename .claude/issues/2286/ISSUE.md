# SCR-D5-NEW5-01: SetMotionType's literal-integer branch reintroduces the #1652 hkpMotion mis-mapping in a new module

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2286
**Source audit**: `docs/audits/AUDIT_SCRIPTING_2026-08-03.md`
**Severity**: HIGH
**Dimension**: Recognizer-Chain Soundness (Dimension 5)
**Location**: `crates/scripting/src/translate/effects.rs:698-709` (`motion_type_arg`, the literal-`IntLit` branch)
**Labels**: high, ecs, bug

## Body

(see GitHub issue for full body — description, evidence, impact, suggested fix, completeness checklist)

Summary: `motion_type_arg`'s literal-`IntLit` branch hardcodes `1 => Dynamic, 4 => Keyframed, 5 => Static, 7 => CharacterKinematic`, which does not match the canonical `hkpMotion::MotionType` table already implemented and tested in `crates/nif/src/import/collision/mod.rs` (`havok_motion_type`: `1..=5|8 => Dynamic, 6 => Keyframed, 7 => Static, 9 => CharacterKinematic`). This is an independent recurrence of the closed #1652 bug pattern in a new module (M47's `SetMotionType` VMAD effect recognizer), reachable because `Motion_*` Papyrus properties are `AutoReadOnly` and decompile as bare integer literals, not named `MemberAccess`.

**Related**: sibling to closed #1652.
