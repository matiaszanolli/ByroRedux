# CHAR-2026-10-26-L-11: The Oblivion Health / Magicka / Fatigue rows are `.player_only()` with no capture line stating their scope

- **Severity**: LOW
- **Dimension**: Derived Formulas (scope contract)
- **Game**: oblivion
- **Location**: `crates/core/src/character/tes.rs:39-41`, `:46-48`, `:83-90`; assumption in `regen.rs:~347`
- **Status**: NEW (same class as closed #2937 and #4450)
- **Source**: absent. `charal-oblivion-ruleset.md` states no player/NPC scope for any pool formula. The justification in `tes.rs` is analogy to the Fallout convention, which is itself FO4-evidence-specific. The sibling Armor Rating rows are `ActorGeneral`, with the capture saying the source is silent (`:344-345`).
- **Impact**: None today. Oblivion has no wired ruleset. It matters when #3768 lands.
- **Suggested Fix**: State the scope as unsourced in `tes.rs` and the capture, and extend the existing scope pin to the Oblivion rows.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-11`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

