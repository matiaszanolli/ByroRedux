# #3502: FO3-2026-08-27-D4-01: the object-LOD descent only subdivides, so FO3's seven level-8-only worldspaces have no distant objects in the near band

**Labels**: medium, terrain-exterior, bug, game:fo3, legacy-compat
**Audit**: `docs/audits/AUDIT_FO3_2026-08-27.md`

---

Source: `docs/audits/AUDIT_FO3_2026-08-27.md` — finding `FO3-2026-08-27-D4-01` (MEDIUM, Dimension 4 — cell loading end-to-end / distant object LOD).

## Location
- `byroredux/src/cell_loader/lod_bands.rs` — `select_lod_quads` (the `too_near || !available(level, qx, qy)` descent, currently ~L325-331)
- `byroredux/src/cell_loader/object_lod.rs` — the missing-quad `ObjectLodBlock::empty()` sentinel (~L218-222)
- `byroredux/src/cell_loader/lod_bands.rs` — `FALLOUT_LEGACY_REFINE_BU` / `FALLOUT_LEGACY_MAX_CELLS` (~L119-120)

## Description
`select_lod_quads` has exactly one escape for a quad whose baked asset is absent — descend:

```rust
            if too_near || !available(level, qx, qy) {
                let half = level / 2;
                stack.push((half, qx, qy));
```

That reasoning holds for **terrain**, where the caller synthesizes a heightmap quad when the baked one is missing. It does not hold for **objects**: `stream_object_lod_blocks`'s fallback is `ObjectLodBlock::empty()` — a "nothing here" sentinel:

```rust
            None => {
                // No baked mesh for this quad — remember so we don't
                // re-extract on every boundary crossing.
                blocks.insert((level, qx, qy), ObjectLodBlock::empty());
            }
```

FNV never exposes this because every FNV worldspace bakes its `blocks\` quads at level 4 — the finest level, which always emits. **FO3 does not.** Archive census of `Fallout - Meshes.bsa` (`meshes\landscape\lod\<world>\blocks\<world>.level<L>.*.nif`, measured during the audit):

```
world           terrain levels                    object levels
dcworld01       {}                                {8: 3}
dcworld03       {4: 64, 8: 16}                    {8: 4}
dcworld06       {}                                {8: 6}
dcworld12       {}                                {8: 8}
dcworld17       {}                                {8: 6}
paradisefalls   {4: 16}                           {8: 1}
washmontop      {4: 112, 8: 28, 16: 7}            {8: 65}
---- (the other eight worldspaces, incl. wasteland 276 quads, are level-4-only) ----
```

Seven of fifteen worldspaces — **93 of FO3's 422 baked object quads, including all 65 of `WashMonTop`'s** — sit at level 8 with no level-4 sibling.

## Evidence
Driving `select_lod_quads` with the FO3 ladder and an availability predicate that admits level 8 only (temporary probe, since reverted):

```
coarsest=32 max_cells=64
refine_threshold(8) = Some(12)   refine_threshold(16) = Some(18)   refine_threshold(32) = Some(27)
exclude_within=4 : by_level={4: 55, 8: 105}  nearest={4: 5,  8: 16}
exclude_within=6 : by_level={4: 48, 8: 105}  nearest={4: 8,  8: 16}
exclude_within=13: by_level={8: 105}         nearest={8: 16}
```

At `exclude_within = 4` (i.e. `--radius 3`, the radius the exterior smoke/bench recipes use) the descent requests **55 level-4 quads spanning cells 5..15**, every one of which is absent on these worldspaces and therefore becomes an `empty()` sentinel; the nearest quad that can actually draw is the level-8 one at 16 cells. The 12-cell figure is `cells_from_bu(FALLOUT_LEGACY_REFINE_BU[0] = 50 000) = 12`.

## Impact
On `WashMonTop`, `ParadiseFalls` and the five level-8-only `DCworld*` worldspaces, every baked distant building disappears from the ~5..15-cell band whenever the streaming radius is below 12 — the visually dominant middle distance, right where the full-detail ring ends. The default `--radius 12` (`byroredux/src/scene.rs`, `.unwrap_or(12)`) sets `exclude_within = radius_unload = 13 > 12`, so the level-4 request never survives the `d > exclude_within` gate and the hole closes. That coupling is accidental and undocumented: nothing ties the default radius to `FALLOUT_LEGACY_REFINE_BU[0]`, and lowering either re-opens the hole silently.

## Related
#3321 (which wired the scheme and did the FNV census — the FO3 per-level split was not part of it). Distinct from #3203 (closed; `LodBandLadder::for_object_game` now returns `fallout_legacy()` for `Fallout3NV`). The residual 2-cell gap at `exclude_within = 13` (nothing between 14 and 15 cells) follows from the deliberate whole-quad containment rule of #1866/#1871 and is cross-game, not an FO3 issue.

## Suggested Fix
Give the object arm a coarsen fallback the terrain arm does not need — when a subdivision produces only unavailable children, emit the parent quad if *it* is available, instead of descending into sentinels. A cheaper stopgap is to derive the descent's finest level per worldspace from the archive inventory (the availability probe is already memoised per `(level, qx, qy)` since #3385) so a level-8-only worldspace never asks for level 4 at all.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the terrain arm of `select_lod_quads`, `placement_lod.rs`, other per-game ladders)
- [ ] **TESTS**: A regression test pins this specific fix (a level-8-only availability predicate must not emit level-4 requests)
