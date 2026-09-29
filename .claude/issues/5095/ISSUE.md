# #5095: SKY-D3-2026-09-29-01: Skyrim's third-person player body has no head — the prebaked FaceGen miss skips the head entirely, but the module doc and the slice doc say it "leaves the race-default head"

**Labels**: bug, medium, legacy-compat, gameplay, game:skyrim

**Source report**: `docs/audits/AUDIT_SKYRIM_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: NPC equip + FaceGen (player body through the prebaked spawn path)

## Location
- `byroredux/src/player_body.rs` (module doc: "the graceful miss leaves the race-default head"; prebaked job for the player).
- `byroredux/src/npc_spawn/resumable.rs`: `skip_missing_facegen` jumps to `PrebakedPhase::Armor(0)`; `PrebakedPhase` runs Skeleton → Facegen → Armor → Finalize with no head fallback; the Facegen phase returns `skip_missing_facegen()` on a missing path or unextractable mesh.
- `docs/engine/playable-vertical-slice.md` (P3 paragraph: "player FaceGen (vanilla ships no facegeom for the player record — the graceful miss leaves the race-default head)").
- `docs/smoke-tests/p3-player-body.sh` (no head assertion).

## Description
On Skyrim the player is assembled through `NpcSpawnJob::prebaked`. Its only head source is `facegeom\skyrim.esm\00000007.nif`, which vanilla does not ship. `skip_missing_facegen` then jumps to `PrebakedPhase::Armor(0)`. The race-default head machinery (`head_part_path` / `head_part_entries`) is called only from `prepare_runtime_state`, the runtime-recipe path. Skyrim's RACE `WNAM` skin ARMO covers body, hands and feet but not the head. The third-person player therefore has no head, eyes, hair or brows.

Both docs describe a different outcome ("leaves the race-default head"). The Skyrim-only gate `p3-player-body.sh` asserts meshes > 0, skeleton, part stamps, first-person hiding and the walk clip, but never a head, so it stays green on a headless body.

## Evidence
- `grep -a -c 00000007.nif` over `Skyrim - Meshes0.bsa` / `Skyrim - Meshes1.bsa` (SE) → 0 / 0, while other facegeom names are present.
- The data a fallback needs is already parsed: Skyrim NPC_ `PNAM` head parts are captured into `face.head_parts` (`crates/plugin/src/esm/records/actor/mod.rs`, `captures_fo4_face = game.uses_prebaked_facegen()`), and HDPT records sit in `index.head_parts`.

Validated at HEAD 9fcfdc3fc: `skip_missing_facegen` sets `PrebakedPhase::Armor(0)`; `head_part_path`/`head_part_paths` callers are all inside `prepare_runtime_state`; both doc sentences present; `p3-player-body.sh` has no head check; BSA byte scan above re-run.

## Impact
Every Skyrim third-person view of the player shows a headless body (the P3 slice route). FO4's player takes the same path, but only Skyrim is gated. The docs present P3 as done except for "player FaceGen", which understates what is visible.

## Related
- GAME-D1-2026-09-29-03 (same doc paragraphs, gear-import clause), `playable-vertical-slice.md` P3 "player FaceGen" open item.

## Suggested Fix
On a prebaked FaceGen miss, assemble a runtime head from the NPC's `PNAM` head parts, falling back to the RACE default HDPTs (via `index.head_parts`) with the race skin tint. At minimum, correct both docs to "no head" and add a head-mesh assertion to `p3-player-body.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same prebaked-miss path checked for FO4's player and for NPCs whose facegeom is missing
- [ ] **TESTS**: A regression test pins this specific fix (prebaked job with missing FaceGen still yields a head part), plus the smoke-script head assertion

