# CHAR-D5-01: CharacterLevel is populated from a PC-level multiplier, not a level — every levelled NPC gets top-tier gear

- **Issue**: [#2955](https://github.com/matiaszanolli/ByroRedux/issues/2955)
- **Finding ID**: `CHAR-D5-01`
- **Labels**: `high,legacy-compat,import-pipeline,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2955 --json state`.

---

- **Severity**: HIGH
- **Dimension**: Population Boundary
- **Game**: fnv, fo3
- **Location**: `byroredux/src/npc_spawn.rs` (`stamp_character_components`) ·
  `crates/plugin/src/esm/records/actor/mod.rs` (`parse_npc_core`, the 24-byte
  `b"ACBS"` arm) · consumed by `crates/plugin/src/equip.rs`
  (`expand_leveled_form_id`) and `crates/scripting/src/condition.rs`
  (`GetXPForNextLevel`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md`, "XP / level curve — LOCKED":
  *"**Level cap:** FO3 **20** (30 with *Broken Steel*); FNV **30** (50 with the four
  add-ons, +5 each)."* A canonical `CharacterLevel.level` of 500–4000 is 17×–200× the
  documented cap, so the value cannot be a level under any reading of the ruleset.
- **Description**: `stamp_character_components` writes
  `CharacterLevel { level: npc.level.max(0) as u16, xp: 0 }` verbatim from the ACBS
  level field. On FO3/FNV that field is overloaded: when the ACBS "PC Level Mult" flag
  is set the field carries a **level multiplier**, not an absolute level. `acbs_flags`
  *is* parsed and stored on `NpcRecord`, but the only bit anything consults is bit 0
  (gender, via `Gender::from_acbs_flags`); nothing in the CHARAL population path — or
  anywhere else — checks the multiplier flag before treating the field as a level.
- **Evidence**: probe over vanilla `FalloutNV.esm` / `Fallout3.esm` via
  `byroredux_plugin::esm::parse_esm`, correlating `NpcRecord::level` against each
  `acbs_flags` bit:

  | | FNV | FO3 |
  |---|---|---|
  | NPC_ records | 3816 | 1647 |
  | `level > 100` | **268** (7.0 %) | **188** (11.4 %) |
  | `acbs_flags & 0x0080` set | 268 | 197 |
  | …of which `level > 100` | **268 / 268** | **188 / 197** |

  The partition on FNV is exact: bit `0x0080` and `level > 100` select the *same* 268
  records. No other bit correlates (bit `0x0010`, the auto-calc bit, covers 2283 FNV
  NPCs of which only the same 268 exceed 100). The out-of-range values are exclusively
  round steps — FNV `{500, 750, 800, 850, 900, 1000, 1100, 1200, 1250, 1300, 2000}`,
  FO3 adds `{600, 1500, 1750, 3000, 4000}` — i.e. a fixed-point multiplier, not a
  level. `1000` alone accounts for 184 FNV and 103 FO3 records.

  Two live consumers read the corrupted value:
  ```rust
  // crates/plugin/src/equip.rs :: expand_leveled_inner
  let eligible: Vec<&_> = lvli.entries.iter()
      .filter(|e| e.level as i32 <= actor_level as i32).collect();
  …
  let pick = eligible.iter().max_by_key(|e| e.level)   // single-pick: highest tier
  ```
  `build_npc_equip_state` seeds `actor_level = npc.level`, so an actor whose "level" is
  1000 makes **every** LVLI entry eligible and always draws the top tier. And
  `GetXPForNextLevel` evaluates `rs.leveling.xp_to_next(1000)` = `150·1000 + 50` =
  **150 050** instead of ~200.
- **Impact**: 268 FNV and ~190 FO3 base actors — the PC-level-scaled population, i.e.
  most generic raiders / troopers / Legionaries, the ones that appear in bulk — carry a
  canonical `CharacterLevel` two to three orders of magnitude wrong. Visible today as
  end-game leveled gear on low-level encounters; latent for every future CHARAL
  consumer, since `DerivedInput::LEVEL` rows, the leveling model, and the M45 save
  snapshot all read this field. The FO3/FNV `LEVEL`-bearing derived rows are
  `player_only`, so `GetActorValue` currently masks the derived-stat half — but
  `pool_regen_tick_system` evaluates `derived_value` *without* the scope gate
  (`CHAR-D1-01` / `CHAR-D2-05`), so that mask is one wiring change from lifting.
- **Related**: `CHAR-D1-01`, `CHAR-D2-05` (unscoped `derived_value`); `#1650`
  (CLOSED — the Oblivion ACBS parse gap, same field, different failure);
  the leveled-list half routes to `/audit-esm` Dim 4 / equipment.
- **Suggested Fix**: gate on the ACBS multiplier flag in `stamp_character_components`
  before writing `CharacterLevel` — when set, the field is a multiplier and the actor's
  level is a function of the player's, which is not modelled yet, so the honest write is
  the ACBS `calc_min` (already in the wire layout, currently skipped) or no
  `CharacterLevel` at all rather than the raw multiplier. **Do not divide by a guessed
  constant**: the `×1000` scale is inferred from the value distribution here, not
  sourced — pin it against xEdit `wbDefinitionsFNV.pas` first
  (`feedback_no_guessing`). The same gate belongs on `build_npc_equip_state`'s
  `actor_level`.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
