# CHAR-2026-10-16-L-10: Stale comments naming moved files and the wrong modifier layer

Labels: low,character,documentation,doc-rot
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW (doc rot)
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `byroredux/src/commands/actor_value.rs:16-18` (`SetAvCommand` doc) and `:253` (test doc `setav_on_player_derived_pool_survives_the_refresh`) say the player-pool redirect "routes into the permanent-modifier layer"; `byroredux/src/combat.rs` (~887, ~901) and `.claude/commands/audit-character/SKILL.md` name `condition.rs` for the `GetActorValue` arm
- **Status**: NEW (the #5708 sweep fixed the four-layer sum prose but not these)
- **Description**: Since #5412 the redirect is `set_override` and the permanent layer is deliberately untouched, so the comments describe the exact bug #5412 fixed. The `GetActorValue` arm lives in `crates/scripting/src/condition/eval.rs` since the split.
- **Suggested Fix**: Say `set_override`; re-point the `condition.rs` references.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).
