# CHAR-2026-10-26-L-15: The player body takes its gender from the shell's ACBS but its race from the Use-Traits terminal

- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: fo3, fnv, oblivion
- **Location**: `byroredux/src/player_body.rs:406` (`Gender::from_acbs_flags(npc.acbs_flags)`) beside `:163-168`
- **Status**: NEW
- **Description**: The race is resolved through `resolve_inherited_traits`, but `attach_player_locomotion_animation` receives the unresolved shell and picks the walk clip from `npc.acbs_flags`. The Female flag is a Use-Traits field. The NPC path got this fix in #6080.
- **Impact**: None for vanilla, since the Player `NPC_` is not templated. A wrong-gender walk clip only for a mod that templates the Player record.
- **Suggested Fix**: Pass the already-computed terminal into `attach_player_locomotion_animation`.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-15`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

