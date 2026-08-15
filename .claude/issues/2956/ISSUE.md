# CHAR-D5-02: CHARAL population ignores every NPC_ template-inheritance flag except "Use Inventory"

- **Issue**: [#2956](https://github.com/matiaszanolli/ByroRedux/issues/2956)
- **Finding ID**: `CHAR-D5-02`
- **Labels**: `medium,legacy-compat,import-pipeline,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2956 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fnv, fo3 (and FO4, which keeps the same TPLT model)
- **Location**: `byroredux/src/npc_spawn.rs` (`stamp_actor_values`,
  `stamp_character_components`) · `crates/plugin/src/esm/records/actor_value_derive.rs`
  (`derive_autocalc_actor_values`) · against `crates/plugin/src/equip.rs`
  (`resolve_inherited_inventory`, `TEMPLATE_FLAG_USE_INVENTORY`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md`, "Inheritance chain (where a given
  NPC's SPECIAL comes from)": *"**`TPLT` + ACBS Template Flags** — if "Use Stats" is
  set, inherit SPECIAL / level / etc. from the template `NPC_`/`LVLN` (FO4 keeps the
  FO3/FNV template model…)"* — item 2 of a four-step chain, ahead of the NPC's own
  overrides. Corroborated by `NpcRecord::template_flags`' own doc comment, which names
  `0x0001` Use Traits and `0x0002` Use Stats.
- **Description**: `NpcRecord::template_flags` parses all twelve bits and the field
  doc enumerates them, but `TEMPLATE_FLAG_USE_INVENTORY` (`0x0100`) is the **only** one
  any code consults — a repo-wide grep for `template_flags` outside tests returns
  `resolve_inherited_inventory` and nothing else. The CHARAL population path reads the
  NPC's *own* `class_form_id` (`derive_autocalc_actor_values` →
  `index.classes.get(&npc.class_form_id)`), its own `level` and its own
  `race_form_id`/`class_form_id` (`stamp_character_components` → `Background`),
  unconditionally. When `Use Stats` (`0x0002`) is set those fields are engine-ignored
  and the authoritative values live on the `TPLT` target.
- **Evidence**: probe over vanilla masters. `Use Traits`/`Use Stats` counts are from
  `template_flags`; divergence is measured against the record at `template_form_id`
  where that resolves to a direct `NPC_`.

  | | FNV | FO3 |
  |---|---|---|
  | NPC_ records | 3816 | 1647 |
  | `template_form_id != 0` | 2573 | 986 |
  | `Use Stats` (`0x0002`) set | **2097 (55.0 %)** | **879 (53.4 %)** |
  | …target is a direct `NPC_` | 1510 | 720 |
  | …target is an `LVLN` | 587 | 159 |
  | **own class ≠ template's class** | **117 / 1510** | **105 / 720** |
  | **own level ≠ template's level** | **86 / 1510** | **56 / 720** |
  | `Use Traits` (`0x0001`) set | 744 | 337 |
  | …own race ≠ template's race | 2 | 19 |

  A differing class is not cosmetic: `derive_autocalc_actor_values` takes
  `class.base_attributes` as the actor's whole SPECIAL and then derives all 15 skills
  from it via `base_skill`, so one wrong class FormID mis-states **22 actor values** on
  that actor. The 587 FNV / 159 FO3 `LVLN`-targeted cases are never resolved at all.
  Note the earlier disproof attempt: shell NPCs do **not** omit `CNAM` — every FNV and
  FO3 NPC_ carries a non-zero, resolvable `class_form_id` (measured: 0 with
  `class_form_id == 0`, 0 unresolvable). So the failure is not "no stats" but
  "stats derived from the record the engine ignores".
- **Impact**: at least 117 FNV and 105 FO3 base actors get a full SPECIAL + 15-skill
  set derived from the wrong class, plus 86/56 with a wrong `CharacterLevel` and
  `Background`. Silent — no log, no fallback, and `GetActorValue` returns a
  plausible-looking number. Every skill-check condition, package gate and future
  combat/dialogue consumer reads it. The engine already paid this exact bug once on the
  inventory axis (`#1658`, templated NPCs spawning naked); the stats axis has the same
  shape and no equivalent resolver.
- **Related**: `#1658` (CLOSED — the inventory half, which is where
  `resolve_inherited_inventory` came from and why the pattern is already proven);
  `CHAR-D5-01` (the other `CharacterLevel` defect, independent).
- **Suggested Fix**: generalise `resolve_inherited_inventory` into a
  `resolve_inherited_stats(npc, index) -> &NpcRecord` that walks `TPLT` (with the same
  `TPLT_MAX_DEPTH` cap and the same `LVLN` tier pick) when `0x0002` is set, and route
  `derive_autocalc_actor_values` + `stamp_character_components` through it; do the same
  for `0x0001` on `Background::race_form_id`. Promote
  `TEMPLATE_FLAG_USE_INVENTORY`'s neighbours to named constants beside it so the bit
  values stay single-sourced from xEdit.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
