# OBL-D2-2026-10-31-01: #6460 bridged four CTDA getters to the ObScript VM but only for FO3/FNV; 535 Oblivion INFOs (493 `GREETING`) are still declined only by them

Labels: medium,esm-plugin,scripting,dialogue,legacy-compat,bug,game:oblivion

Filed from `docs/audits/AUDIT_OBLIVION_2026-10-31.md` (finding `OBL-D2-2026-10-31-01`).

- **Severity**: MEDIUM
- **Dimension**: 2 (DIAL/INFO selection). Mechanism owner: `/audit-scripting` (condition catalog); the Oblivion leg of closed (issue 6460) and of FNV-2026-10-27-01. Related, not the same as FO3-2026-10-31-01 / FNV-2026-10-31-03 (those are FO3/FNV getters the catalog still lacks).
- **Location**:
  - `crates/scripting/src/obscript_cmd.rs:156-158`: `getter_answers_conditions` is `Game::of(world) == Game::Fallout && find(cmd, Game::Fallout).is_some()`, and `run_getter_for_condition` (`:164-178`) always runs `find(cmd, Game::Fallout)`.
  - `crates/scripting/src/condition/eval.rs:95-113`: `is_modeled` routes every `vm_getter_command()` function through that gate.
  - `crates/scripting/src/condition/catalog.rs:548-558,764-768`, `crates/scripting/src/condition.rs:98-107`: rows and table say "(FO3/FNV)".
- **Status**: NEW for Oblivion. The `#6460` commit says the ten getters "stay declined (as before) on Oblivion, Skyrim and FO4" because the ids "were aligned with the indices of" FO3/FNV. That does not hold for four of them.
- **Description**: Four of the ten bridged getters are the same function at the same index on Oblivion, and the ObScript command table already answers them there:
  - UESP *Oblivion Mod/Function Indices*: 50 `GetTalkedToPC`, 18 `GetCurrentTime`, 32 `GetInSameCell`, 5 `GetLocked`.
  - Their command rows are `Games::Both`: `0x1032` (`obscript_more.rs:553`), `0x1012` (`CMD_GET_CURRENT_TIME`), `0x1020` (`CMD_GET_IN_SAME_CELL`), `0x1005` (`CMD_GET_LOCKED`), all in `obscript_effects/getters.rs`.
  - The fail-closed dialogue gate declines an INFO whose CTDA function `is_modeled` rejects, so on Oblivion these INFOs can never be selected, although the VM answers the same getter for scripts.
- **Evidence** (`/tmp/audit/oblivion/py/ctda.py`: 20-byte TES4 headers, zlib records inflated, CTDA function at +8; modeled set = catalog `ROWS` indices minus the ten bridged getters, plus `OBLIVION_DIALECT`):
  - 3,079 of 19,278 Oblivion INFOs are declined (same as 10-27).
  - **535** are declined only by the four getters: `GetTalkedToPC` 502, `GetCurrentTime` 22, `GetInSameCell` 12, `GetLocked` 1.
  - By topic: **`GREETING` 493**, `HELLO` 26, and 16 others (`JauffreTopic` 4, `CountessValga` 2, `CountessCarvain` 2, `GOODBYE` 1, ...).
  - Every Oblivion activation opens through `GREETING` (#5367), so the first-meeting greetings (`GetTalkedToPC == 0`) are the ones that never select.
- **Impact**: 493 `GREETING` lines (the first-meeting greetings) can never be chosen, so the affected NPCs fall through to another line or none. Interiors and the Tamriel exterior are unaffected.
- **Suggested Fix**: Make `getter_answers_conditions` / `run_getter_for_condition` resolve the row for the loaded game (`Game::of(world)`), not `Game::Fallout`, and pass `fallout = game == Game::Fallout` to `run_row`. A getter with no row for the loaded game (`find` returns `None`) still declines, which keeps the other six bridged functions as they are on Oblivion (confirm each has no `Games::Both` row before relying on it). Keep 365 on the dialect (`GetPlayerInSEWorld`). Pin: the Oblivion declined-INFO count falls from 3,079 to 2,544 (`condition_catalog_real_data.rs` has the FO3/FNV pins to extend).
- **Note on the rest of the Oblivion gap** (not findings): sole-blocker INFOs after this fix are `GetDisposition` (76) 1,341, `GetScriptVariable` (53) 545, `GetIsCurrentPackage` (161) 82, `GetGold` (48) 61, `GetPCIsRace` (130) 35, `GetTrespassWarningLevel` (144) 31. The FO3 catalog finding (FO3-2026-10-31-01) covers the shared rows.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
