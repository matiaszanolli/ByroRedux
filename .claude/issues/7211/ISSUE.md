# #7211 CHAR-2026-11-21-L-14: "The Player `NPC_` is form 7" is restated as a literal in three population paths beside the canonical constant

**Labels**: low,character,tech-debt,bug

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D4-05
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: all (Oblivion-added copies)
- **Location**: `crates/plugin/src/esm/records/oblivion_population.rs:62` (`resolved.shell.form_id == 7`), `byroredux/src/npc_spawn/stamps.rs:103` (`index.npcs.get(&7)`), `crates/scripting/src/oblivion_skill_use.rs:74` (`const PLAYER_NPC`); canonical: `byroredux/src/inventory/mod.rs:42,71` (`PLAYER_NPC_FORM_ID`, `player_npc_form_id`, with a per-game test `player_npc_form_id_is_canonical_across_supported_games`)
- **Status**: NEW
- **Source**: n/a (duplication, not a numeric rule)
- **Description**: three new Oblivion-era sites each hard-code the lineage-wide Player base id instead of one shared definition. `inventory/mod.rs` documents that an exhaustive per-game table is there so a divergent variant is a compiler-visible edit; the copies in the plugin and scripting crates bypass it (the plugin and scripting crates cannot see a `byroredux` binary item, so the constant belongs lower).
- **Impact**: no behavior today; a future master whose Player differs would be fixed in one site and silently missed in three.
- **Suggested Fix**: hoist the constant to a crate both sides see (the plugin crate next to `NpcRecord`) and have `player_npc_form_id` and the three sites use it. Per the repo rule, improve the existing definition rather than adding a fourth.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
