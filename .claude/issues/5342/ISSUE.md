# #5342 — CHAR-2026-10-05-D5-01: #5238 replaced rather than split the capture's Health row — the FO3/FNV player Health formula and its (player) scope vanished from charal-fnv-fo3-ruleset.md, and five code/doc sites describe the old state

- **Labels**: low,character,doc-rot,game:fnv,game:fo3,documentation
- **Filed from**: `docs/audits/AUDIT_CHARACTER_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5342

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location and evidence**:
  1. `docs/engine/charal-fnv-fo3-ruleset.md`, the derived table. The row `| Health | … | 90 + END·20 + Level·10 | 100 + END·20 + (Level−1)·5 | BUILT (player ruleset + NPC auto-calc seed) |` became `| Health (NPC) | … |`. The player formulas that `fallout3_ruleset` / `falloutnv_ruleset` encode, with their bias 90/95 and `.player_only()` scope, now have no row in their own capture. They survive only in `charal-fo4-ruleset.md`'s cross-game table, which is labelled "file into the sibling rulesets when opened". The prose keeps only the multipliers.
  2. `crates/core/src/character/fallout.rs`, the `fallout3_ruleset` AP-row comment: "`charal-fnv-fo3-ruleset.md`'s derived-stat table annotates scope on every other locked row (Health "(player)", …)". The doc of `fo3_fnv_action_points_scope_is_player_only_pending_a_source` likewise says "Health's player-only scope, which the capture document does state". Both are now false.
  3. `crates/plugin/src/esm/records/actor_value_derive.rs`, the module doc `## Model (cited)`: "**Health**: FO3 `90 + 20·END + 10·Level`; FNV `100 + 20·END + 5·(Level−1)`, from the locked CHARAL ruleset capture". That is the retired NPC curve; #5238 updated the function but not this header. The "unparsed" claim in the same doc belongs to D4-01.
  4. The pin count is overstated. The `NpcHealthCurve` doc, the capture paragraph and the audit skill all say "15/15 … pinned by `fo3_fnv_npc_health_curve_reproduces_the_vanilla_samples`", but that test asserts 8 samples (5 FNV + 3 FO3). The real-master `#[ignore]` test asserts 3.
  5. `byroredux/src/inventory.rs`, the player-template test message "FNV Health 95 + 20·END(5) + 5·L(1), **the NPC curve** the player record's class derives". The 200 is the `PlayerOnly` row; the NPC curve gives 20.

  Also, `ROADMAP.md` ("CHARAL runtime-dead surface") still opens with "The formulas are right: 62 constants were verified with zero mismatch", citing 2026-08-15. #5238 falsified that for the NPC Health row.
- **Status**: NEW (`c71eeed80`, `297a0e64a`).
- **Impact**: The next auditor looking for the source of FO3/FNV player Health finds none in its own capture, and code comments assert a capture annotation that is gone. This is the "a row with no document value is UNSOURCED" trap, which bites only because the row moved.
- **Suggested Fix**:
  - Restore a `Health (player)` row beside `Health (NPC)`, carrying the two player formulas and the "(player)" scope. Cite the fandom *Hit Points* page and the authored `fAVDHealth*` GMSTs (FO3 20/10, FNV 20/5, endurance offset 0.0).
  - Fix the `actor_value_derive.rs` header, the two `fallout.rs` comments and the `inventory.rs` message.
  - Change "15/15 pinned" to "8 pinned + 3 real-master", or add the remaining 7 samples to the pin.
  - Soften the ROADMAP sentence.

_Source: `AUDIT_CHARACTER_2026-10-05.md` (CHAR-2026-10-05-D5-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
