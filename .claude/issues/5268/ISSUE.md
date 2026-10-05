# #5268 — GAME-D1-2026-10-05-02: #5028's gear-release path has no production producer, and its "still held" filter contradicts the zero-in-place row convention

- **Labels**: low,gameplay,inventory,tech-debt,bug
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5268

- **Severity**: LOW (a tested-but-unwired path; latent)
- **Dimension**: 1 / 7 (written-never-read class)
- **Location**:
  - `byroredux/src/npc_spawn/loot_appearance.rs:293-392` (`queue_gear_releases`; the filter is at `:359-372`)
  - `byroredux/src/inventory.rs:921-934`, `:1011-1023` (the only producer of `added: false`)
- **Status**: NEW. #5028 is closed.
- **Description**:
  - `queue_gear_releases` acts only on an `ItemEventBatch` row with `added == false` for a wearer **without** `CellRoot`, which in practice means only the player.
  - The only production producer of `added: false` is `transfer_loot`, and it emits it on the *source*. `transfer_loot` refuses `player == source` (`:927`).
  - `pickup_loot` emits only `added: true`. The scripting `Effect`s add items but never remove them (`fragment/effects.rs:770-880`). No drop, sell or `RemoveItem` path exists.
  - So the player can never produce the event the release path waits for. Its doc comment claims "drop, sell, destroy". The six `release_*` tests insert the event batch by hand.
  - Separately, when a producer does land, the "still held" test `inventory.items.iter().all(|stack| stack.base_form_id != form_id)` (`:364-367`) checks whether a row is *present*. Rows are zeroed in place, never removed (`LootSelection::Stack`, `consume_item`), so a zero-count row of the same base would block the release. The filter should test `stack.count > 0`, the same way `reconcile_worn_gear` does at `:528`.
- **Impact**: none today. An unequipped mid-life piece stays resident and hidden, by design. The ECS finding ECS-2026-10-05-D7-01 (a despawned gear root left in the body root's `Children`) is latent for the same reason.
- **Suggested Fix**: correct the doc to say the path is forward-latent until a player-side removal exists. Change the filter to count only rows with `count > 0`. Add a test with a zeroed same-base row.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D1-2026-10-05-02), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
