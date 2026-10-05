# #5269 — GAME-D1-2026-10-05-03: #5058 inserted reset_player_factions_to_record between reconcile_player_equipped_weapon's doc comment and its fn — rustdoc attaches the #3488 weapon doc to the faction reset

- **Labels**: low,gameplay,doc-rot,documentation
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5269

- **Severity**: LOW (documentation; the same class #5227 fixed elsewhere)
- **Dimension**: 1
- **Location**: `byroredux/src/inventory.rs:1629-1697`
- **Status**: NEW
- **Description**:
  - The 25-line `///` block beginning "Rebuild the runtime [`EquippedWeapon`] consequence…" (`:1629-1653`) now runs straight into the #5058 block ("post-load faction reset…") and documents `reset_player_factions_to_record`.
  - `pub(crate) fn reconcile_player_equipped_weapon` (`:1697`) is left with no doc at all, and the doc it lost is the one that explains the additive-overlay contract that the reconciler exists for.
- **Suggested Fix**: move the #5058 block and its function above `:1629`, or below `reconcile_player_equipped_weapon`.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D1-2026-10-05-03), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
