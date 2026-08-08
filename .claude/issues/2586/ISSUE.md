# SK-D6-01: LOD quad origin assumes worldspace-independent alignment -- 9 of 12 vanilla Skyrim worldspaces resolve zero .bto/.btr

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2586
**Finding ID**: SK-D6-01

**Severity**: HIGH
**Dimension**: Specialty Blocks + Real-Data Rendering
**Location**: `byroredux/src/cell_loader/object_lod.rs:385-400` (`quad_origin`/`bto_archive_path`); `terrain_lod.rs:273-277,367-380`; `terrain_lod_btr.rs:72-75`
**Status**: NEW (adjacent to open epic #2371, which covers *missing coarse bands* — a different defect)

## Description
Both LOD path builders derive the quad's SW-corner cell as `cell.div_euclid(level) * level`, assuming every worldspace's LOD quad grid is aligned to absolute multiples of `level`. The vanilla Skyrim SE filename corpus disproves this: each worldspace tiles from its **own** grid origin, generally non-zero. The module's own doc comment asserts the wrong rule, citing only Tamriel filenames as evidence — the one worldspace where the assumption happens to hold.

## Evidence
Confirmed directly: `object_lod.rs:385-386` — `pub(crate) fn quad_origin(gx: i32, gy: i32, level: i32) -> (i32, i32) { (gx.div_euclid(level) * level, gy.div_euclid(level) * level) }`. All 10,662 `.bto`/`.btr` names in `Skyrim - Meshes1.bsa` parse against the expected path pattern (0 unmatched). Per-worldspace `(x mod level, y mod level)` at level 4 is a single non-zero constant for 10 of 12 worldspaces. Reachability at level 4 (the only band either loader requests): **5,735 of 7,897 files resolvable (72.6%)** — Blackreach, Deepwood Redoubt, Falmer Valley, Soul Cairn, Hunter HQ, Apocrypha, Japhet's Folly, Skuldafn all resolve **zero**; Markarth resolves 169/194. Across all levels: 3,074 of 10,662 files (28.8%) unreachable.

## Impact
Nine of twelve vanilla worldspaces — including Apocrypha (Dragonborn's main questing space, 1,063 LOD files) and the Soul Cairn (Dawnguard, 944) — get **zero distant object LOD**, permanently and silently: `spawn_object_lod_quad` misses caches an `ObjectLodBlock::empty()` sentinel with no log line and, unlike terrain, no synth fallback. Distant terrain in those worlds degrades to the flat-texture synth block (visible quality loss, not a blackout). Tamriel and Solstheim work, which is exactly why this survived the EXAL step-6 verification.

## Related
#2371 (distant LOD bands epic), #1866 (ring/hysteresis gating), #2086 (same "verified on one title, generalised to all" failure class)

## Suggested Fix
Derive the quad grid origin per worldspace instead of assuming `(0,0)` — `LODSettings\<World>.lod` and the `WrldRecord` min-cell both carry it. Replace `quad_origin(gx, gy, level)` with an origin-relative version and thread the same origin into `terrain_lod.rs`'s block index. Add a regression test using real non-Tamriel filenames (`dlc2apocryphaworld.4.-50.-50.btr`, `dlc01soulcairn.4.-52.-51.btr`).

## Completeness Checks
- [ ] **TESTS**: A regression test using real non-Tamriel filenames (`dlc2apocryphaworld.4.-50.-50.btr`, `dlc01soulcairn.4.-52.-51.btr`) confirms correct resolution
- [ ] **SIBLING**: `terrain_lod.rs`'s block index threaded with the same per-worldspace origin, not just `object_lod.rs`
