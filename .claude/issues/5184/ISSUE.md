# #5184: EXT-D5-2026-10-02-04: The sentinel guard resolves both "per-game" records under `GameKind::Skyrim`, so it cannot catch a game-keyed sentinel now that the translate takes a game

**Labels**: low,terrain-exterior,water,test-gap,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**: `byroredux/src/env_translate.rs:3937-3938` (`resolve_water_material_sentinels_are_game_invariant`)
- **Status**: NEW (a test gap opened by #4910's signature change)
- **Tier Violated**: no-render-time-fallback (guard coverage)
- **Game Affected**: all
- **Description**:
  - Before #4910, `resolve_water_material` had no game input, so a game-dependent sentinel was structurally impossible and the guard only had to vary the records.
  - #4910 added `game: GameKind`, and the guard now passes `GameKind::Skyrim` for both the "Oblivion-shaped" and the "Skyrim-shaped" record.
  - A future `match game { … }` that sets a sentinel field such as `ior`, `uv_scale_*` or `shoreline_width` inside the translate would pass this plain guard.
  - The only test that varies the game is the real-data `water_sentinels_hold_on_real_watr_per_game`, which is `#[ignore]` (`:4123`). The new `wind_angle_conversion_is_scoped_per_game` sweeps games, but only for layer motion.
- **Evidence**:
  ```rust
  let (ob, ob_kind, ob_flow, _, _) = resolve_water_material(&waters, Some(0x0001_0000), GameKind::Skyrim);
  let (sk, _, _, _, _) = resolve_water_material(&waters, Some(0x0002_0000), GameKind::Skyrim);
  ```
- **Impact**: The guard named "game_invariant" no longer exercises the game axis it is named for.
- **Suggested Fix**: Resolve the sentinel records under every `GameKind`, at least resolving the Oblivion record under `GameKind::Oblivion`, and assert that the §4 sentinel list is identical across them.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
