# UI-D7-2026-09-29-02: the #4675 fix left three HUD doc comments describing the retired Skyrim-keyed FormID bar source

**Labels**: low,documentation,doc-rot,ui

**Source report**: `docs/audits/AUDIT_UI_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: MenuXml / both
- **Location**: `byroredux/src/hud.rs:40-43` (`HudBar`), `byroredux/src/hud.rs:779-786` (`bar_fractions`),
  `byroredux/src/hud.rs:808-810` (`fraction`)
- **Status**: NEW. 65717ab58 updated `ui.md` but not these comments.
- **Description**: the three comments still describe the old bar source.
  - `HudBar`: "Skyrim-profile AVIF keys the engine already stamps (`0x3E8` Health, `0x3E9` Magicka, `0x3EA` Stamina)…
    the Oblivion HUD reads these opportunistically."
  - `bar_fractions`: "any stamped actor values on an actor entity… today the Oblivion profile reads the Skyrim-keyed
    values… FO3/FNV read their own AVIF keys."
  - `fraction`: "Skyrim uses the same AVIF keys."

  Since #4675, every profile resolves editor ids through `PlayerVitals` and reads the player only. Oblivion resolves no
  ids today (`build_player_vitals`, #3768/#4679), so its bars stay full unless pinned.
- **Impact**: documentation only. It describes exactly the "first stamped actor / literal FormID" behaviour that #4675
  removed, which invites it back.
- **Related**: #4675 (closed), #4679
- **Suggested Fix**: Rewrite the three comments to the editor-id → `PlayerVitals` → `PlayerEntity` contract, and state
  the Oblivion limitation.

**Validated at HEAD 9fcfdc3fc**: `byroredux/src/hud.rs` still has the `HudBar` doc naming `0x3E8` Health etc., the `bar_fractions` doc saying "the Oblivion profile reads the Skyrim-keyed values", and the `fraction` doc saying "Skyrim uses the same AVIF keys".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
