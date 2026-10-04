# #5239: CHAR-2026-10-03-D1-01: `setav` and the SDK's `SetBase` on the player's derived pools (FO3/FNV/FO4 Health + AP) report success and are silently reverted one frame later by #5039's per-frame refresh

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: medium,character,bug,game:fo3,game:fnv,game:fo4
- **Source report**: docs/audits/AUDIT_CHARACTER_2026-10-03.md

- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam (the refresh contract) / Population Boundary (the skill's "`setav`/`modav` write the *base* component, not a derived output the next tick recomputes" check)
- **Game**: fo3, fnv, fo4
- **Location**:
  - `crates/core/src/character/ruleset.rs`: `CharacterRuleset::refresh_player_only_bases`, which re-stamps whenever `base_authored && base != value` is false.
  - `byroredux/src/systems/character.rs`: `player_derived_stats_system`, which runs every `Stage::Update` frame.
  - The writers: `byroredux/src/commands/actor_value.rs` (`AvEdit::SetBase`) and `byroredux/src/extensions/commands.rs` (`ActorValueOperation::SetBase`).
- **Status**: NEW (`c9254beb8`, 2026-09-30, the #5039 fix). Before it the stamp was static, so `setav Health` stuck.
- **Source**: the GECK *SetActorValue* note: "For the player, this will not modify base health … If you use player.SetAv Health 100 the player will have 180 total health — 80 from base health, and 100 for the rest." The arithmetic is inherited from the CS wiki (`fPCBaseHealthMult`). The structural claim is what matters: a player SetAV on a derived pool persists alongside the formula-derived base and is not replaced by it.
- **Description**: The refresh treats the authored base as a pure cache of the formula. Any authored base that differs from the formula output is overwritten. `set_base` is also the only way a console command, a script-facing SDK command or a save writes a base, so a deliberate player SetAV on Health/AP is indistinguishable from a stale stamp.
- **Evidence**: A throwaway test in a HEAD worktree used the FO4 player Health row, END 5, L1:
  ```
  setav 0x2D4 on entity 0: 105 -> 500     (command output)
  after_cmd=500  after_tick=105           (one player_derived_stats_system call)
  ```
  `charal.md` §6 lists "`setav`/`modav`" among the writers the refresh serves. That is true for writes to the *inputs* (END/AGI). Writes to the *output* key are never mentioned and no test covers them.
- **Impact**:
  - The engine console's `setav . <Health> N` and every SDK mod issuing `SetBase` on the player's Health or AP are no-ops that report success. The SDK path even reads the mutated state back and returns it to the sandbox.
  - Debug and repro workflows that set the player's HP silently fail.
  - Save/load is unaffected: the refresh recomputes the same base.
- **Related**: #5039 (its fix introduced this); #5042 (`base_authored`); D4-01 (a separate defect on the same Health rows).
- **Suggested Fix**: Keep the formula and the user-set value apart. One option is to have the refresh write only the formula-owned portion: compare against the last *stamped* formula output, which needs one stored value, and leave any `SetBase` delta in place. Another is to route a `SetBase` on a `PlayerOnly` output into the permanent-modifier layer at the write sites. At minimum, have `setav` refuse or warn on a `PlayerOnly` output key, and document the behaviour in `charal.md` §6.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (console `setav`, SDK `ActorValueOperation::SetBase`, save restore path)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, `CharacterRuleset` → `ActorValues` order is preserved
- [ ] **TESTS**: A regression test pins this specific fix (setav on a PlayerOnly output survives `player_derived_stats_system`)

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-10-03.md` (finding CHAR-2026-10-03-D1-01, /audit-character 2026-10-03, HEAD `2c36c29d8`).*
