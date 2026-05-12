# Oblivion Compatibility Audit — 2026-05-11 (Dimension 3 focus)

**Scope**: Dimension 3 — ESM Record Coverage (TES4 / Oblivion).
**Depth**: deep, real-data validation against `Oblivion.esm`.
**Method**: orchestrator + single `general-purpose` agent.
**Baseline**: 30 open GitHub issues, all renderer-focused (no ESM overlap).

## Executive Summary

- **Findings**: 0 CRITICAL, 1 HIGH, 4 MEDIUM, 2 LOW.
- **Headline**: The TES4 parser is **far from a stub** — CLAUDE.md's "legacy/tes4.rs" framing is stale (the module was removed in #390/#690). `EsmVariant` auto-detects Oblivion via the offset-20 `HEDR` literal, branches the 20- vs 24-byte record/group header at every read site, and the records dispatcher already handles ~80 fourCCs including SCPT (pre-Papyrus bytecode), ENCH, MGEF, SPEL, RACE, CLAS, CONT, LVLC, NPC_, CREA, WTHR, CLMT, REGN, DIAL/INFO, plus 31 minimal-stub fourCCs. CELL XCLL is correctly length-gated for the Oblivion 28-byte / FNV 40-byte / Skyrim 92-byte variants.
- **Top blocker for exterior render**: `parse_wrld_group` only extracts `EDID` + `CNAM` — every other WRLD field (NAM0/NAM9 grid bounds, PNAM parent worldspace, DATA flags, OFST per-cell LAND offsets, ICON map texture, MNAM map data) is dropped. **One HIGH finding + a two-line CELL `has_water` bit are the only structural ESM-side changes needed to unblock Tamriel exterior render.**
- **Stale framing to retire**: "TES4 parser is a stub" (CLAUDE.md, `legacy/mod.rs` lines 27-32 document this). Update on next CLAUDE.md sync.

## Findings

### HIGH — WRLD record drops every exterior-bound and map-data field
- **Premise**: `parse_wrld_group` matches exactly two sub-records — `EDID`, `CNAM`. Everything else falls to `_ => {}` — [crates/plugin/src/esm/cell/wrld.rs:47-61](crates/plugin/src/esm/cell/wrld.rs#L47-L61).
- **Gap**: Missing for Oblivion: `WCTR` (center cell coords), `NAM0` + `NAM9` (usable grid bounds, min/max XY), `NAM2` (default water height), `ICON` (map texture), `MNAM` (map data block), `PNAM` (parent worldspace + inheritance flags), `DATA` (flags: no-LOD-water, small-world, fixed-dimensions, no-grass), `XEZN` (encounter zone), `SNAM` (default music), `RNAM` (region overrides), `OFST` (per-cell LAND offset table).
- **Impact**: Cannot bound the Tamriel exterior grid (no NAM0/NAM9), cannot resolve parent-worldspace inheritance (Shivering Isles DLC needs PNAM), cannot stream LAND in radius (no OFST), cannot pick the right map texture or default music. **Single biggest exterior-render blocker.**
- **Fix**: Add `WorldspaceRecord { usable_min: (i32, i32), usable_max: (i32, i32), parent_worldspace: Option<u32>, parent_flags: u8, default_music: Option<u32>, water_height: f32, map_texture: String, flags: u8 }`, populate from the sub-records, store on `EsmCellIndex.worldspaces`. OFST is a follow-up (LAND-streamer consumer).
- **Dedup**: New.

### MEDIUM — Oblivion-unique base records BSGN, CLOT, APPA, SGST, SLGM silently skipped
- **Premise**: No dispatch arm for these fourCCs in [crates/plugin/src/esm/records/mod.rs:675-1262](crates/plugin/src/esm/records/mod.rs#L675-L1262); they fall to the catch-all `_ => { reader.skip_group(&group); }` at line 1259-1261.
- **Gap**: BSGN (birthsigns, 13 vanilla, ties to SPEL list); CLOT (clothing, distinct in TES4, folded into ARMO from FO3 on); APPA (alchemical apparatus, 4 vanilla); SGST (sigil stones, Oblivion-gates); SLGM (soul gems, ENCH charge cross-ref).
- **Impact**: No rendering impact (none carry placement REFRs). Affects gameplay subsystems: birthsign auto-spell, clothing tier display, ENCH→SLGM cross-refs.
- **Fix**: Add 5 dispatch arms with `MinimalEsmRecord` storage (matches the #810 long-tail pattern). SLGM gets a single `soul_capacity: u8` field from DATA byte 0.
- **Dedup**: New; same shape as #458 / #810.

### MEDIUM — `parse_race` is FNV-shape only; Oblivion RACE DATA differs substantially
- **Premise**: [crates/plugin/src/esm/records/actor.rs:545-558](crates/plugin/src/esm/records/actor.rs#L545-L558) reads exactly 7 × `(u32, i8)` skill-bonus pairs at DATA offset 0, plus MODL.
- **Gap**: Oblivion RACE DATA carries 7 skill bonuses (compatible) + 14 × u8 base attributes per gender + base height/weight (4 × f32) + flags (u32) + voice forms + default hair/eyes. Sub-records `XNAM` (race-vs-race reactions), `VNAM`, `DNAM`, `CNAM`, `PNAM` (face-morph values), `UNAM`, `FNAM`, `INDX` per body part are all dropped.
- **Impact**: Default scale, default hair/eyes, voice routing silently dropped. Doesn't block render. M41.0 Phase 3b FaceGen recipe needs this.
- **Fix**: Thread `game: GameKind` into `parse_race`, branch on Oblivion to extend `RaceRecord` with the base attributes / hair / eyes / voice fields. FNV path stays untouched.
- **Dedup**: New.

### MEDIUM — `parse_clas` reads FNV's 35-byte DATA; Oblivion CLAS DATA is 60 bytes with different semantics
- **Premise**: [crates/plugin/src/esm/records/actor.rs:587-601](crates/plugin/src/esm/records/actor.rs#L587-L601) gates on `len >= 35`, reads 4 × u32 tag skills at 0..16 and 7 × u8 attribute weights at offset 28.
- **Gap**: Oblivion CLAS DATA is 60 bytes: 2 × u32 *primary attribute pair* (not 7 weights), u32 specialization (combat/magic/stealth), 14 × u32 major skills (skill-AVIF indices, different semantic from FNV tag skills), flags u32, services u32, i8 trainer skill, u8 trainer level, 2 bytes padding. Plus an `ICON` portrait sub-record.
- **Impact**: Every Oblivion CLAS parses but populates `attribute_weights` with mid-skill-list bytes — meaningless values. No render impact. Blocks any class-aware gameplay logic.
- **Fix**: Thread `game: GameKind` into `parse_clas`, branch to a 60-byte Oblivion layout. Extend `ClassRecord` with `Option<(u32, u32)>` for the primary attribute pair + specialization.
- **Dedup**: New.

### MEDIUM — Oblivion MGEF lookup is keyed wrong: 4-char EDID codes vs FormID
- **Premise**: [crates/plugin/src/esm/records/misc.rs:709-726](crates/plugin/src/esm/records/misc.rs#L709-L726) parses MGEF and the consumer indexes `EsmIndex.magic_effects: HashMap<u32, MgefRecord>` by FormID.
- **Gap**: Oblivion's engine looks up effects by their literal 4-byte EDID code ("FIDG", "DGFA", "REDG", "DRSP", …), and SPEL/ENCH/ALCH cross-refs via `EFID` carry the 4-byte code, **not** a u32 FormID. The current FormID-keyed map can't resolve these references on Oblivion.
- **Impact**: When spell-casting / enchant runtime lands, every Oblivion spell silently no-ops. Zero impact today (consumer doesn't exist yet).
- **Fix**: Add a secondary `magic_effects_by_code: HashMap<[u8; 4], u32>` map (code → FormID) populated when `game == GameKind::Oblivion`. Defer until SPEL/ENCH consumers materialize.
- **Dedup**: New; tied to spell/enchant runtime, not current renderer path.

### LOW — `parse_qust` doesn't decode SCDA/SCHR/CTDA/QSDT/QSTA stages or objectives
- **Premise**: [crates/plugin/src/esm/records/misc.rs:478-491](crates/plugin/src/esm/records/misc.rs#L478-L491) matches EDID/FULL/SCRI/DATA-first-2-bytes only.
- **Gap**: QSDT (stage indexes), CTDA (per-stage conditions), QSTA (objective targets), SCDA (stage-completion scripts), INDX (stage indices), CNAM (stage log text) all silently dropped. Cross-game (FNV reference also has this gap).
- **Impact**: No quest data round-tripped beyond identity. No bearing on rendering.
- **Fix**: Defer; pre-existing acknowledged gap in `QustRecord` doc comment.
- **Dedup**: Already documented in code as deferred.

### LOW — Oblivion RCLR (cell regional-color override) never parsed
- **Premise**: No `b"RCLR"` arm in [crates/plugin/src/esm/cell/walkers.rs:85-242](crates/plugin/src/esm/cell/walkers.rs#L85-L242).
- **Gap**: Oblivion exterior CELL records can carry RCLR (3-byte RGB override of regional fog/sky color). FO3+ moved this to LGTM/CLMT chain.
- **Impact**: Editor-authored cell-level color tint dropped (rare in vanilla, occasional in mods). No effect on renderer defaults.
- **Fix**: Add `CellData.regional_color_override: Option<[u8; 3]>` + 3-byte read in the cell walker, gated on Oblivion.
- **Dedup**: New; minor.

## Minimum Path to Exterior Render

Starting state: interior cells already render end-to-end (Anvil Heinrich Oaken Halls); the records dispatcher already populates `cells.statics` / `cells.exterior_cells` / `cells.landscape_textures` / `cells.worldspace_climates` on Oblivion; the cell loader picks `"tamriel"` by EDID at [byroredux/src/cell_loader.rs:650](byroredux/src/cell_loader.rs#L650); LAND/VHGT/VNML/VCLR/BTXT/ATXT/VTXT decode is cross-game and works; LTEX Oblivion-ICON direct path is wired ([crates/plugin/src/esm/cell/support.rs:180-185](crates/plugin/src/esm/cell/support.rs#L180-L185)); WTHR HNAM 56-byte and CLMT WLST 8-byte are correct (#537 / #540).

1. **WRLD grid bounds (HIGH fix).** Add `WorldspaceRecord` with at least `NAM0` + `NAM9`. Cell loader iterates `(min_x..=max_x, min_y..=max_y) ∩ requested_radius` instead of guessing from explicit cells. **Highest leverage.**
2. **WRLD parent + flags.** Add `PNAM` (parent FormID + inheritance flags) and `DATA` (flags byte). Required for Shivering Isles and parent-inheriting modded worldspaces.
3. **CELL DATA bit `0x02` (has water).** Currently only the interior bit `0x01` is decoded ([crates/plugin/src/esm/cell/walkers.rs:96](crates/plugin/src/esm/cell/walkers.rs#L96)). Two-line fix.
4. **LAND verification test.** `parse_land_record` is cross-game; add one ignored test that parses a Tamriel exterior cell and asserts a non-zero heightmap. Belt-and-braces.
5. **OFST table.** Performance optimization for LAND streaming. Defer.

The HIGH WRLD fix + the two-line CELL `has_water` bit are the only **structural ESM-side changes** needed to unblock Tamriel exterior render. Everything else for visual parity (LAND, LTEX, climate, weather, REFR placement, XSCL, XESP, ownership) is already wired and tested for Oblivion.

## Regression Guards (verified still correct)

- **#391** Oblivion 20-byte group-header over-read. `group_content_end` is variant-aware — [crates/plugin/src/esm/reader.rs:538](crates/plugin/src/esm/reader.rs#L538); test `group_content_end_is_variant_aware` at `reader.rs:1005-1043`.
- **#396** ACRE (Oblivion creature placement) routes through REFR/ACHR walker — [crates/plugin/src/esm/cell/walkers.rs:309-311](crates/plugin/src/esm/cell/walkers.rs#L309-L311).
- **#439** FO3↔FO4 HEDR band inversion. Oblivion variant-dispatched independent of HEDR float — [crates/plugin/src/esm/reader.rs:111](crates/plugin/src/esm/reader.rs#L111); test `game_kind_from_header_maps_real_master_hedr_values`.
- **#445** FormID multi-plugin collision: identity remap for single-plugin Oblivion works — [crates/plugin/src/esm/reader.rs:256-278](crates/plugin/src/esm/reader.rs#L256-L278).
- **#458** REGN/ECZN/LGTM/HDPT/EYES/HAIR/NAVI/NAVM/WATR all round-trip on Oblivion — [crates/plugin/src/esm/records/mod.rs:913-948](crates/plugin/src/esm/records/mod.rs#L913-L948).
- **#537 / #540** Oblivion HNAM 56-byte + WLST 8-byte stride; `parse_rate_oblivion_esm` pins ≥30 weathers + ≥25 non-default fog — [crates/plugin/tests/parse_real_esm.rs:756-826](crates/plugin/tests/parse_real_esm.rs#L756-L826).
- **#566** LTMP lighting-template fallback cross-game including Oblivion — [crates/plugin/src/esm/cell/walkers.rs:118](crates/plugin/src/esm/cell/walkers.rs#L118).
- **#631** DIAL+INFO walker (`extract_dial_with_info`) handles the Topic Children nested sub-GRUP layout for TES4→FO4 — [crates/plugin/src/esm/records/mod.rs:1417-1455](crates/plugin/src/esm/records/mod.rs#L1417-L1455).
- **#685 / #686 / #691** Oblivion WEAP 30-byte / ARMO 14-byte / AMMO 18-byte DATA layouts, with regression tests — [crates/plugin/src/esm/records/items.rs:820-1000](crates/plugin/src/esm/records/items.rs#L820-L1000).
- **#690 / #390** `legacy/tes4.rs` stub removed; working parser lives in `crates/plugin/src/esm/`. Confirmed: `crates/plugin/src/legacy/` contains only `mod.rs`.
- **#692** XOWN / XRNK / XGLB cell + REFR ownership cross-game — [crates/plugin/src/esm/cell/walkers.rs:140-148](crates/plugin/src/esm/cell/walkers.rs#L140-L148).
- **#693** Pre-Skyrim XCMT 1-byte music enum, Skyrim XCCM CLMT override; XCMT correctly tagged Oblivion / FO3 / FNV branch — [crates/plugin/src/esm/cell/walkers.rs:123-130](crates/plugin/src/esm/cell/walkers.rs#L123-L130).
- **#810** Minimal-stub long-tail pattern adopted; 31 audio/visual/hardcore/caravan/casino fourCCs round-trip.

## Notes

- `Oblivion.esm` first 64 bytes confirmed: `TES4` magic + 20-byte record header + HEDR at offset 0x14 + version `0x3F800000` (= 1.0f32). Dispatch is by offset-20 `HEDR` literal, not by float — robust against the FO3 0.94 vs FO4 1.0 collision.
- The `parse_rate_oblivion_esm` ignored end-to-end test exists at [crates/plugin/tests/parse_real_esm.rs:731-826](crates/plugin/tests/parse_real_esm.rs#L731-L826) — run with `BYROREDUX_OBLIVION_DATA=…` to validate ESM coverage after each fix.
- All 7 findings are net-new vs the 30 open GitHub issues (all of which are renderer-focused).

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-05-11_DIM3.md`
