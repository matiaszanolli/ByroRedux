# CHAR-2026-10-26-L-14: `KeywordTable::from_index` reads race (and FO4 keywords) off the placed shell, outside the #6228 pin

- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: skyrim, fo4
- **Location**: `crates/scripting/src/keywords.rs:40-49`
- **Status**: NEW (same class as closed #6228; related to open #4137)
- **Description**: `index.races.get(&npc.race_form_id)` takes the raw `NpcRecord`'s race, not the Use-Traits terminal's. `HasKeyword` on a templated actor reads the shell's race keywords. The reader landed 2026-10-13 (`5bf9ac87e`), before #6228, and was missed by that sweep. The pin lists only dialogue voice, `actor_base_traits` and `anim_subgraph`.
- **Impact**: Latent, and only where a templated NPC's own RNAM disagrees with its terminal. Not measured on a real master.
- **Suggested Fix**: Resolve `traits_terminal(npc, index)` per NPC and read `race_form_id` from it. Add `scripting/keywords.rs` to the pin's file list.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-14`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

