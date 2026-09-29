# CHAR-2026-09-29-D4-02: Player population reads TPLT-governed fields three different ways, and `player_body.rs` adds a production `resolve_inherited_traits` call that the #4457 guard does not scan

**Labels**: low,bug,character,test-gap

**Source report**: `docs/audits/AUDIT_CHARACTER_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: all seeded families (latent on vanilla)
- **Source**: not numeric. The rule is the #4457 contract: consumers read the `ResolvedNpc` terminals, and a raw `npc.<field>` read or a new direct call is the recurrence.
- **Location**:
  - `byroredux/src/inventory.rs:368-372`: `effective_actor_level(player)`, `player.race_form_id` and `player.class_form_id` read from the shell.
  - `byroredux/src/inventory.rs:577,588,607`: `build_player_template_for` reads `effective_actor_level(player)` and `player.default_outfit`.
  - `byroredux/src/player_body.rs:160-164`: `resolve_inherited_traits(&npc, …)`.
  - `byroredux/src/npc_spawn/tests.rs:2612-2696`: the guard's six-file list.
- **Status**: NEW (`a070baaad` 2026-09-28; `7b0b84c2c` #4678)
- **Description**: The NPC path takes each field from its template terminal:
  - `Background` from `resolved.r#traits` / `resolved.stats`, and `CharacterLevel` from `resolved.stats` (`stamp_character_components`).
  - Outfit and gear level from the Use-Inventory / Use-Stats terminals (#4812).

  The player path builds a `ResolvedNpc`, then reads level, race, class and outfit from the raw shell. Separately, the player body resolves its race through `resolve_inherited_traits`. So one templated Player record would get a body race from the terminal and a `Background` race from the shell.

  `player_body.rs` is not among the files `resolve_inherited_call_sites_are_enumerated_and_pinned` counts. The enumerated production count moved from 1 to 2 with the guard still green.
- **Evidence**: The probe finds Player `NPC_` 0x7 with `tplt=0x0 tflags=0x0` on FNV, FO3, FO4 and Skyrim, so every read agrees on vanilla data today.
- **Impact**: None on vanilla content. A mod that templates the Player record would get inconsistent level, background and outfit. The guard gives false assurance about the site count.
- **Related**: #4457, #4812, #4678, #4137
- **Suggested Fix**: Read `resolved.stats` / `resolved.r#traits` / the inventory terminal in both player templates. Add `player_body.rs` to the guard (expected 1, as the pre-spawn race boundary, like `cell_loader/references/mod.rs`), or pass it the `ResolvedNpc` the job already builds.

**Validated at HEAD 9fcfdc3fc**: `byroredux/src/inventory.rs` still reads `effective_actor_level(player)`, `player.race_form_id`, `player.class_form_id` and `player.default_outfit` from the shell; `byroredux/src/player_body.rs` calls `resolve_inherited_traits`; the `resolve_inherited_call_sites_are_enumerated_and_pinned` guard's `include_str!` list does not include `player_body.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
