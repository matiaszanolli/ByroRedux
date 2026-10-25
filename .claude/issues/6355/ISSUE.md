# TD1-2026-10-25-01: `crates/scripting/src/obscript_effects.rs` crossed 2000 prod LOC (1537 -> 2034, +497 in two commits) — a result of the #6139 table conversion, not a grown-back split hub

Labels: low, scripting, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6355
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 1 · **Effort**: medium
- **Age**: `da22354f1` (stage 6, 2026-10-24: 1528 -> 1892, `run` 780 lines / 63 arms became 63 handlers + `EFFECT_COMMANDS`) and `6561576ab` (stage 7: 1892 -> 2032, `getter` 310 lines / 36 arms became 43 handlers + `GETTER_COMMANDS`). Two 2026-10-24 commits since then add +2. Was NOT on the 10-21 list (1537). This is the "newly crossed" case: the file was never a split hub (no `obscript_effects/` dir exists; `git log` on the path is a plain history, 15 commits since 10-18). #6139 (CLOSED) removed the 6 giant `match cmd` functions; their bodies landed here as ~106 small `fn(world,_fallout,cmd,args,context) -> Action` handlers.
- **Location**: `crates/scripting/src/obscript_effects.rs` (2951 total; prod 1-2034, inline `mod tests` L2035-2951 with 44 tests)
- **Structure read** (no fn >176 lines; this is breadth, not depth): module doc L1-48 · CMD id consts L64-83+ (effect ids) · `Binder`, `adjust_standing`, `form_arg`/`num`/`live` helpers L120-228 (these four are imported by 6 sibling files: `obscript_more/{extras,state,tail,actions}.rs`, `obscript_more.rs`, `obscript_quests.rs`) · mutating handlers L229-1359 (~1130 lines) · `finish_effects` L1360 + `EFFECT_COMMANDS` table L1370-1454 · getter id consts L1457-1535 · getter handlers L1537-1987 (~450) · `GETTER_COMMANDS` L1988-2032.
- **Split axis (handlers by responsibility; the two tables are the seam)**: convert to `obscript_effects/`: `mod.rs` (docs, `Binder`, `form_arg`/`num`/`live`/`adjust_standing`, `finish_effects`, and `pub(crate)` re-exports so the 8 `crate::obscript_effects::{form_arg,live,num,EFFECT_COMMANDS,GETTER_COMMANDS,adjust_standing,CMD_*}` call sites are unchanged) · `getters.rs` (L1457-2032: id consts + 43 getter handlers + `GETTER_COMMANDS`) · effect handlers split by the domain the table rows already read in: `inventory.rs` (add_item, equip/unequip, remove_all*, place_at_me), `combat.rs` (kill, enemy/ally, start/stop combat, alarm, unconscious/ghost, resurrect, reset_ai, restrained, alert, assault alarm), `actor_state.rs` (set/mod AV, essential, perk, faction/rank/reputation, reward_*, teammate), `world_state.rs` (lock, open state, destroyed, delete, ownership, weather, set_pos/angle/scale, play_group), `presentation.rs` (say, show_message, conversation, sound, idle, imod, magic shaders, cast, dispel) each with its own `COMMANDS` slice concatenated in `mod.rs`. Tests move to `obscript_effects/tests.rs` (precedent in-tree: `obscript_more/tests.rs`).
- **Hazards**: no `include_str!` reader of this file (grep clean). `obscript_cmd.rs:94-95` lists the two tables in `FAMILIES`; keep the `duplicate_rows` test green by keeping the same `&[Command]` slices (or listing the new family slices). The two row tables are order-insensitive (duplicate-id test) so splitting is safe.
- **Related**: #6139 (closed, caused the move), #5804 (condition.rs, same "table conversion concentrates rows in one file" shape).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD1-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
