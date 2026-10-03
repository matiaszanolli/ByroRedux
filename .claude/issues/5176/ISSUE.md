# #5176: EXT-D3-2026-10-02-04: Over budget (#4920), plants are kept by rank in residency-slot order, so whole late-slot chunks go bare instead of thinning evenly

**Labels**: low,terrain-exterior,shaders,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW (visual; only past the cap)
- **Dimension**: Ground-cover pipeline
- **Location**:
  - `crates/renderer/shaders/groundcover_models.comp:366-397` (LAYOUT grants) and `:438-439,465-467` (EMIT `rank < count`).
  - `byroredux/src/render/groundcover.rs:360-375` (chunks arrive in slot order).
  - `docs/engine/exal-groundcover.md` §12.12 "Over budget".
- **Status**: NEW (side effect of #4920's fix, not covered by it)
- **Tier Violated**: n/a
- **Game Affected**: all, once demand exceeds the 32,768-instance tail
- **Description**:
  - A plant's rank is its record's running count across chunks in `gcChunks` order, which is residency-ring slot order (`reconcile`), not distance.
  - `floor(t_r × cap / demand)` keeps each record's first N plants by rank. The plants dropped are therefore whole spatial runs of late-slot chunks, which can be next to the camera.
  - When the camera moves, slots are reassigned and the bare regions jump.
  - The doc says "every record keeps the same fraction of its plants". That is true per record, but it does not mention the spatial effect.
- **Evidence**: The code path cited above. EXT-D3-01 raises demand about 3×, so the cap is more likely to bind than when #4920 was verified (FNV `demanded=50`).
- **Impact**: Bare patches near the player once Skyrim/FO4 density crosses the cap. Not measured.
- **Suggested Fix**:
  - Admit by a frame-invariant per-plant hash threshold. Keep a plant if `hash(point) < cap / demand`, so thinning is uniform and deterministic.
  - Or rank chunks nearest-first before granting.
  - Record the chosen policy in §12.12.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
