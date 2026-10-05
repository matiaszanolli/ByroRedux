# #5335: EXT-D5-2026-10-05-02: Oblivion never gets distant water — `spawn_lod_water` needs WRLD `NAM3`/`NAM4`, which Oblivion does not author, although #5243's per-cell model has every input it needs

**Labels**: medium,terrain-exterior,water,game:oblivion,legacy-compat,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5335

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-05.md` — `EXT-D5-2026-10-05-02` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM (visible missing content on every Oblivion exterior water vista beyond the streaming radius)
- **Dimension**: Water translation (WATAL); distant water
- **Location**:
  - `byroredux/src/streaming.rs:950-982` (`spawn_lod_water`: `let (Some(height), lod_water_form) = translate_lod_water(..) else { return; }`).
  - `byroredux/src/env_translate.rs:239-276` (`translate_lod_water`, whose doc says Oblivion has neither field).
  - Compare `env_translate.rs:208-237` (`default_water_for_worldspace` gives Oblivion Z = 0 when NAM2 is present).
  - Docs: `docs/engine/watal.md:788-790` ("Older games naturally use the same path").
- **Status**: NEW. It predates #5243: the single-sheet era had the same gate. But #5243's census table lists Oblivion Tamriel as "99% inherit … coastal thousands wet-at-distance" and calls it "the least-affected worldspace", while the engine draws no distant water for it at all. Searched open and closed issues for "Oblivion distant water", "LOD water": no tracker.
- **Tier Violated**: n/a (a coverage gap; the canonical inputs exist)
- **Game Affected**: Oblivion
- **Description**:
  - The per-cell builder needs only the cell table (XCLW tri-state, LAND) plus a default height and a WATR form for the appearance.
  - Oblivion has both, through the full-detail resolver `default_water_for_worldspace`: sea level 0 when `NAM2` is authored, and `NAM2` as the form.
  - `spawn_lod_water` takes its default and form only from `translate_lod_water` (`NAM4`/`NAM3`), and returns early when they are absent. On Oblivion that is every worldspace.
- **Evidence**:
  - Census of `Oblivion.esm` WRLD: Tamriel, SEWorld and every test world author `NAM2` and no `NAM3`/`NAM4`/`DNAM`.
  - The test `translate_lod_water_is_none_when_unauthored` (`cell_loader/water.rs:2113`) pins the `None`. No test pins what Oblivion should draw instead.
  - #5243's census (`.claude/issues/5243/ISSUE.md`) gives Oblivion Tamriel 14,563 inherit cells at default 0, plus ~123 override lakes at 500–5200.
- **Impact**:
  - Beyond `radius_unload`, Lake Rumare, the Niben, Topal Bay and every override lake show as dry LOD lake bed, including from the Imperial City vistas.
  - The flagship Oblivion water is invisible at distance. Vanilla draws the world water plane to the horizon.
- **Related**: EXT-D5-2026-10-05-03 (same height-source question); #5243.
- **Suggested Fix**:
  - When `NAM3`/`NAM4` are absent, fall back to `default_water_for_worldspace` for the default height and the form. That is one table-shaped rule at the translate boundary, not a game branch in the streamer.
  - Pin it with an Oblivion-shaped fixture: NAM2 only, inherit cells wet at 0.
  - Correct watal.md §5.2.

## Publisher note

Cross-referenced (not re-filed) by `AUDIT_OBLIVION_2026-10-05.md`, which calls it the most visible Oblivion exterior gap today. Coordinate with EXT-D5-2026-10-05-03: the Oblivion fallback must use `default_water_for_worldspace`, not copy the builder's inline `NAM4`-default rule.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
