---
description: "Deep audit of the ESM/ESP plugin parser — GRUP walk, sub-record byte accounting, per-record schemas, FormID remap, CELL/WRLD walkers, ESM→ECS handoff"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# ESM / Plugin Parser Audit

Audit `crates/plugin/` (~62k LOC in `src/`) as a parser: GRUP walker, sub-record byte
accounting, per-record schema dispatch, FormID load-order remap, CELL/WRLD walkers, and
the `EsmIndex` → ECS handoff. Per-game audits (`/audit-fnv`, `/audit-skyrim`, …) each
sample one game's slice; this skill owns the parser itself.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` for
shared protocol (dedup, methodology, finding format). Do not duplicate them here.

## Scope

**Crate**: `crates/plugin/src/` — `esm/reader.rs` (`EsmReader`, `EsmVariant`, `GameKind`,
`FormIdRemap`, `GlobalSlot`, zlib decompression), `esm/sub_reader.rs` (`SubReader`),
`esm/records/` (`parse.rs` = the top-level GRUP walker `parse_esm[_with_load_order]`;
`mod.rs` is a re-export barrel; `index.rs` `EsmIndex`; eight `dispatch_*.rs` routers;
`grup_walker.rs`; per-type decoders incl. `actor/`, `items.rs`, `items/consumable.rs`,
`container.rs`, `condition.rs`, `weather.rs`, `load_screen.rs`, `misc/*.rs`),
`esm/cell/` (`walkers.rs`, `support.rs`, `wrld.rs`, `helpers.rs`, `mod.rs` data types),
`esm/strings_table.rs`, `equip.rs`, and the Redux-native tier
(`datastore`/`manifest`/`record`/`resolver`).

**Not in scope**: cell-loader consumption of `CellData` (`/audit-<game>` Dim 1); VMAD
translation (`/audit-scripting`); NIF (`/audit-nif`); BSA/BA2/CSG incl. the FO4 previs
header → cell-grid recovery (`/audit-parsers`); `src/extension.rs` + `crates/sdk` extension
manifests (`/audit-tooling`); WTHR/CLMT/WATR/LTEX/GRAS *translation* and HNAM sun
consumption (`/audit-exterior` — byte-level decode of those records stays here);
consumption semantics of `src/consumables.rs` / `expand_leveled_loot` (`equip.rs`) —
timed restoratives, container loot, LSCR selection (`/audit-gameplay`; the decode contract
stays here, Dim 4); CHARAL formulas (`/audit-character`).

**Ground truth**: `docs/engine/plugin-loading.md`, `docs/engine/esm-records.md`,
`docs/engine/pipeline-overview.md`. Every non-obvious decode should cite a
`wbDefinitions{TES4,FNV,TES5,FO4,FO76,SF1}.pas:line` range (crate docstring in
`crates/plugin/src/lib.rs`); a decode with no citation and no test is a guess
(*feedback_no_guessing*) — flag it.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers. Default: all 8.
- `--game <name>`: restrict Dim 8 real-data validation to one game. Default: every game
  with on-disk data (`_audit-common.md` § Game Data).
- `--depth shallow|deep`: `shallow` = schema/contract check; `deep` = byte-level trace
  against real masters. Default `deep`.

## Extra Per-Finding Fields

- **Dimension**: Header & GRUP Walk | Sub-Record Byte Accounting | FormID & Load Order |
  Record Schema Dispatch | CELL / WRLD Walkers | Localized Strings | ESM→ECS Handoff |
  Real-Data Validation
- **Record / Sub-record**: 4-char code(s) (e.g. `NPC_` / `ACBS`), or `—`.

## Phase 1: Setup

1. Parse `$ARGUMENTS`. `mkdir -p /tmp/audit/esm`.
2. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/esm/issues.json`.
3. Read the latest `docs/audits/AUDIT_ESM_*.md` plus the ESM dimensions of the latest
   `AUDIT_{FNV,SKYRIM,FO4,STARFIELD}_*` reports — this crate's findings historically live
   there, so **that is where your duplicates are**.
4. `cargo test -p byroredux-plugin`; record the pass count (a pre-existing failure is
   context, not a finding). **Never run `--ignored` / `--include-ignored` on this crate**
   — the real-data tests parse whole masters (Starfield ≈ 4 GB parsed) and have
   OOM-killed sessions past 20 GB; use Dim 8's single-file probes instead.

**Suggested emphasis** (yield across the 9 prior reports): Localized Strings, ESM→ECS
Handoff, CELL/WRLD, FormID remap. Header/GRUP is mostly guarded; byte accounting has low
report yield but no width guard — spot-check decoders changed since the last report.

## Phase 2: Dimensions

### Dimension 1: Header Detection & GRUP Walk (highest blast radius)
Paths: `crates/plugin/src/esm/reader.rs`, `esm/records/{parse,grup_walker}.rs`
First step: `grep -rn 'group_content_end(' crates/plugin/src | grep -v 'tests/\|///'`
**Why first**: a header-size or GRUP-bounds error desyncs the whole file; everything
downstream decodes structurally valid garbage.
**Guards** (run, confirm live — not `#[ignore]`d, not vacuous):
`reader::tests::game_kind_from_header_maps_real_master_hedr_values`,
`grup_walker::tests::every_recursive_grup_walker_honours_the_shared_depth_cap` (table of
walkers nested past `MAX_GRUP_NESTING_DEPTH`; a walker missing from the table is invisible
to it), `reader::tests::bounded_group_content_end_clamps_to_parent_end`, and the
`compressed_record_*` / `inflation_ceiling_clears_every_observed_vanilla_shape` tests.
**Checklist** (what the guards cannot see):
- `EsmVariant::detect` (`data[20..24] == b"HEDR"` ⇒ 20-byte Oblivion headers) has its
  length guard before the slice; no caller hardcodes a variant outside tests /
  `with_variant`.
- `GameKind::from_header(variant, hedr_version, record_version)` uses **banded** float
  compares plus the TES4 record version as a second key (Skyrim LE and FO3 GOTY both stamp
  HEDR 0.94 → record version 40 vs 2; FO4 DLC 0.95 → 131). Vanilla HEDR: FO3 0.94, FO4 1.0,
  Starfield 0.96, FNV 1.34, Skyrim SE 1.71, **FO76 — a live-service value that drifts with patches** (68.0 → 266.0 → 279.0 as of 2026-09-20; floor 60.0). A mis-band is not
  latent: FO4-only arms in `items.rs` (`ARMO DATA` swaps value/weight/health order at the
  same 12-byte length, `WEAP DATA` empty, `BOOK` 8-byte) would silently swap armor stats
  and zero every weapon. Any new per-game schema split widens this blast radius — check.
- Missing/short HEDR falls back to `GameKind::Fallout3NV` (`Default`): confirm each call
  site is deliberate and no branch treats "defaulted" as "detected FO3".
- **Universal GRUP bound** (#3503/#3721/#4076): every self-recursive walker takes
  `bounded_group_content_end(header, depth, parent_end, name)` and threads `depth + 1`
  into its `_inner`. The raw `group_content_end` may appear only in the non-recursive
  top-level loop (`records/parse.rs`) and `cell/wrld.rs::parse_wrld_group` (which must
  `.min(end)` itself). Regression = a new recursive walker on the raw accessor, a dropped
  `depth` argument, or a call site that loses the `parent_end` clamp (the depth guard does
  not test the clamp per site).
- `FLAG_COMPRESSED`: `data_size >= 4` checked before the subtraction; declared size is
  bounded by `record_inflation_ceiling` (`min(64 MiB, max(64 KiB, compressed×512))`, from a
  census of 133k vanilla records — worst real ratio 102:1) and the decoder is held to
  `take(declared + 1)`; mirrors `byroredux_bsa::safety::inflate_bounded` (#3410).
  Regression = either bound removed or the two implementations diverging.
**Output**: `/tmp/audit/esm/dim_1.md`

### Dimension 2: Sub-Record Byte Accounting (densest bug class)
Paths: `crates/plugin/src/esm/sub_reader.rs`, `esm/records/**/*.rs`, `esm/cell/support.rs`
First step: `git log --since=<last report> --name-only --format= -- crates/plugin/src/esm/records | sort -u` (decoders touched since the last report), then re-derive their widths from the cited xEdit range.
ESM analogue of `/audit-nif` Dim 1: a wrong field width shifts every later field and the
record still "parses" with wrong numbers.
**Guard**: `records::test_support::tests::no_module_redeclares_a_local_subrecord_builder`
(fixture hygiene only — nothing here guards decode widths).
**Checklist**:
- Decoders read through `SubReader`; flag direct raw-slice indexing.
- The `*_or_default` family swallows short reads: fine for optional tails, a finding
  mid-fixed-layout (silent zeros). Classify every call site.
- Fixed-size sub-records check the declared length against the schema; version-gated tails
  are gated on `GameKind` or remaining length with an xEdit citation.
- Repeating rows (`CTDA`, `CNTO`, `NVTR`/`NVEX` → `decode_nvtr_row`/`decode_nvex_row`,
  `LVLO`): `len / stride` with an explicit remainder check; a non-zero remainder is a
  decode bug.
- `rgb_color` (3 B) vs `rgba_color` (4 B): swapped pairs shift every later byte. Colours
  are raw monitor-space floats — never flag a missing sRGB conversion (*feedback_color_space*).
- Known offset maps — verify against the code, not older reports: `parse_race` TES5
  `DATA` starting pools at 36/40/44 (new fields land at ≥48; TES4/FO3NV and FO4/FO76 arms
  differ); WATR `decode_data_fo3nv` shared 0..28 prefix (wind speed/direction/amplitude/
  frequency at 0/4/8/12, direction degrees-on-the-wire converted once), pinned by
  `parse_watr_186_byte_record_reads_colors_at_40_44_48` and
  `wind_direction_converts_shipped_degrees_on_the_fo3nv_dnam_arm` (this field moved twice).
- Strings: terminator consumed exactly once; a missing terminator cannot read past the
  sub-record.
- FO76 split leveled entries: `LVLO` (4 B ref) + float `LVLV`/`LVIV` companions are
  decoded only under `GameKind::Fallout76` (`parse_leveled_list_for_game`); a truncated
  legacy `LVLO` must never become a valid reference.
**Output**: `/tmp/audit/esm/dim_2.md`

### Dimension 3: FormID Remap, Load Order & ESL Space
Paths: `crates/plugin/src/esm/reader.rs` (`FormIdRemap`, `GlobalSlot`), `esm/records/tests.rs`, `esm/records/common.rs`, `byroredux/src/cell_loader/load_order.rs`
First step: `cargo test -p byroredux-plugin -- every_record_parser_takes_a_remap parsers_that_take_a_remap no_parser_with_a_remap remap_fid_has_exactly_one`
**Guards**: `records::tests::all_record_parsers` walks `records/` at test time and requires
every `pub fn parse_*` to take a `remap` parameter or sit in `EXEMPT_NO_U32_READS`
(mechanically re-verified) / `EXEMPT_JUSTIFIED` (written non-FormID reason);
`parsers_that_take_a_remap_actually_use_it`; `remap_fid_has_exactly_one_definition`;
`no_parser_with_a_remap_in_scope_passes_an_identity_remap`. The `cell/` tier is guarded
structurally (remap is a required parameter of `read_form_id`). Review **new exemption
entries** in the diff — each is a human decision, not a proof.
**What the guards cannot see — the class keeps recurring in three shapes**:
(1) an unremapped field inside a *shared* decoder several parsers call
(`CommonNamedFields::from_subs_with_remap`, #4067); (2) a GRUP **label** that encodes a
FormID (DIAL topic-children label vs remapped DIAL id, #4079 — check every label-dispatch
site like a sub-record field); (3) a `HashMap<u32, _>` in `EsmIndex` keyed by, or compared
against, a raw plugin-local id.
**Checklist**:
- `GlobalSlot::Regular` keeps 24 bits; `GlobalSlot::Light` (`0xFE` space) packs a 12-bit
  sub-index and keeps only the low **12** object-id bits (`0x0FFF` masks on both sides).
  ESH `0xFD` slots are not modelled — a real gap since the legacy bridge was deleted (#4384).
- `FormIdRemap::remap` arms: raw `0` → `0` (null sentinel, must not become a live ESL id);
  self-reference (`mod_index == master_slots.len()`); in-range master; standalone with no
  masters (pass through, `debug` — the vanilla Oblivion `0x01` artifact, #1308; must NOT be
  "fixed" into a clamp — two forms would collide in `EsmIndex`); out-of-range with masters
  (pass through, `warn`).
- Master list read in file order; a missing master fails diagnosably, not as a silently
  shifted slot table.
- Multi-plugin is live (`--master` repeatable → `parse_esm_with_load_order`); `parse_esm`
  passes `None`. Verify every embedded FormID in a newly-touched parser goes through
  `remap_fid`.
**Output**: `/tmp/audit/esm/dim_3.md`

### Dimension 4: Record Schema Dispatch & Coverage
Paths: `crates/plugin/src/esm/records/{parse,index,items,container,actor_value_derive,load_screen}.rs`, `records/dispatch_*.rs`, `records/misc/*.rs`, `crates/plugin/src/equip.rs`
First step: `cargo test -p byroredux-plugin -- dispatch_handled_fourccs every_index_map_is_a_category`
**Guards**: `records::tests::dispatch_handled_fourccs_matches_the_live_dispatch_arms`
(`DISPATCH_HANDLED_FOURCCS` ↔ the live match in `parse.rs`; era-gated arms count as
handled, `PDCL` is deliberately absent, LCTN has no top-level arm);
`index::tests::every_index_map_is_a_category_or_a_recorded_exclusion` (every `EsmIndex`
map is a `categories()` row or a reasoned exclusion); `cell::plugin_loading_doc_pin_tests::the_documented_esm_cell_index_lists_every_field` and
`…::the_audio_record_split_matches_the_dispatcher` (docs ↔ code).
**Checklist**:
- Build the live matrix per record type: typed decode / minimal stub
  (`parse_minimal_esm_record`, EDID+FULL — `dispatch_misc_stub.rs` also hosts typed
  decoders: `LSCR`, `CLOT`/`CCRD`/`CMNY` → `items`) / skipped. Report the delta against
  *record_type_catalog* as coverage, not bugs. A stub must advance the reader correctly;
  the unknown-record path skips by declared size and never `warn`s per record on a vanilla
  master.
- Per-game schema splits within one record (`ARMO/WEAP/AMMO DATA`, `BOD2` vs `BMDT`,
  `SCOL/PKIN/TXST`, FO76 `LVLO`) switch on `GameKind`. **`ACRE` is not Oblivion-only**
  (FO3 ships 3,349 `ACRE` vs 2,154 `ACHR`) — flag any new per-game gate on it.
- **Consumables / items decode contract** (decode only; consumption is `/audit-gameplay`):
  `ALCH` effect chains via `parse_alch_for_game`; `MgefRecord.instant_restoration_av` via
  `parse_mgef_for_game`; Oblivion `parse_clot` → `ItemRecord`; `IMOD`/`CCRD`/`CMNY`
  carried as items; `LVLI` multi-pick (flag `0x04`) vs single-pick highest-eligible,
  pinned on the shipped FNV master. `expand_leveled_loot` retains unknown terminal ids and
  saturates counts — check it agrees with the decoder's `LeveledEntry` semantics.
- **`LSCR`** (`parse_lscr`, header flags via `extract_records_with_flags`): `LNAM` stores
  grid **Y before X**; malformed known fields land in `malformed_fields` rather than
  producing an unrestricted screen; truncated fixed fields are safe and visible.
- **`TPLT` "Use Stats"** resolves each absent stat *down the chain* to the deepest record
  that authors it (`resolve_inherited_field`, #4086; `TPLT_MAX_DEPTH` 6) — the terminal
  outranks a shallower record and a one-hop chain is unchanged. Check new stat fields join
  the per-field walk instead of comparing only the chain ends.
- `EsmIndex::merge_from`: last-write-wins matches Bethesda override semantics; `game` is
  only adopted when `other.total() > 0` (a failed-parse `EsmIndex::default()` must not
  relabel the load order as FO3/FNV, #3403); `character_rules` is first-non-`NONE` wins.
- `actor_value_derive.rs` is the CHARAL feed — formulas belong to `/audit-character`.
- Settled decodes — verify they hold, do not re-derive: `INAM` is one FormID array (#3356);
  `CREA CNAM` is not a class (#3383); `FACT` rank ladder (#3338); `ARMO` contributes every
  race-matching `ARMA` (#3357), `MOD3` is the female mesh (#3414); a `REFR` tombstone
  removes the placement wherever it lives (#3362); FO76 `HEDR` is patch-dependent — 279.0 on the 2026-09-20 patch, never pin it (#3405, #4643).
- `equip.rs::main_body_bit` FO76/Starfield arms are a **provisional inference**
  (`PROVISIONAL (#4074)`, pinned by `fo76_and_starfield_arms_are_marked_provisional`) —
  known and marked; report only if the marker is removed without an xEdit citation.
**Output**: `/tmp/audit/esm/dim_4.md`

### Dimension 5: CELL / WRLD Walkers & Placement Data
Paths: `crates/plugin/src/esm/cell/**/*.rs`, `esm/records/misc/world.rs`
First step: `git log --since=<last report> --format='%h %cs %s' -- crates/plugin/src/esm/cell crates/plugin/src/esm/records/misc/world.rs`
**Checklist**:
- Cell child groups: type 6 is the `Cell Children` *container*; 8 = Persistent, 9 =
  Temporary, 10 = Visible Distant nest inside it (#4169 — earlier legends said 6/8/9).
  `PlacedRef.group_type` is re-derived at each nested GRUP in `parse_refr_group_inner`
  (only the nested position accepts 10; at CELL/WRLD level 10 is Quest Children). No
  consumer reads it yet — verify it stays populated per-scope, not that a consumer exists.
- All three walkers depth-bounded (see Dim 1): `parse_cell_group`, `parse_wrld_children`,
  `parse_refr_group`.
- Lighting-template inheritance is per-field; "absent" and "authored zero" stay distinct.
- **XCLL** is size- and game-validated: `xcll_canonical_sizes(game)`; the ≥92-byte
  ambient-cube arm fires only for Skyrim/FO4/FO76 (Starfield has its own ≥108 arm) — a non-canonical 92+ XCLL on
  Oblivion/FO3NV keeps base + per-field reads and warns (guard: the cell test
  `parse_cell_non_canonical_92plus_xcll_on_pre_skyrim_games_stays_out_of_the_ambient_cube`).
  Parsed-but-unconsumed tail fields are parked in `docs/engine/lighting-from-cells.md`.
- **LIGH** `LightData`: `falloff_exponent` `0.0` is a sentinel resolved per layout
  generation at the translation boundary (`canonical_light_falloff_exponent`,
  `byroredux/src/systems/light_anim.rs`) — the parser must store authored values
  verbatim. Starfield light shape is the `DAT2+56` Light Type enum
  (`starfield_light_type`: 0 omni, 1 shadow spot, 2 non-shadow spot); `0x200` is *Focus
  Spotlight Beam*, not a shape bit. `DAT2` is gated on LIGH (an AMMO `DAT2` must not
  synthesize light data).
- `XCLW` water height is tri-state: absent → inherit WRLD default; finite → override;
  `INT_MIN` / `FLT_MAX` sentinel → suppress (`xclw_water_height` +
  `water_height_is_explicit`; `docs/engine/watal.md`).
- Exterior grid `XCLC`, worldspace parenting and selective-inheritance flags (`wrld.rs`):
  a child inherits only the flagged categories.
- `parse_land_record`: quadrant/layer counts and splat rows are fixed-stride. An `ATXT`
  with no following `VTXT` must flush with `alpha: None` (`pending_atxt`, #4078;
  measured 14× on `Oblivion.esm`). `LTEX.GNAM` (grass) is decoded in `parse_ltex_group`.
- Starfield `TXST`: an unmodelled slot warns once (#4438), not per record.
- Navmesh: classic `NVTR`/`NVEX` and packed `NVNM` (`decode_nvnm`, `NVNM_MAX_DIVISOR`)
  yield the same `NavmRecord` through shared row decoders; the cursor refuses to pass the
  blob end; retained-raw-on-failure is diagnosable, not a silent success.
**Output**: `/tmp/audit/esm/dim_5.md`

### Dimension 6: Localized Strings (Skyrim+ `.STRINGS` family)
Paths: `crates/plugin/src/esm/strings_table.rs`, `esm/records/common.rs` (`StringsTableGuard`, lstring readers), `byroredux/src/cell_loader/load_order.rs` (table loading)
First step: `cargo test -p byroredux-plugin strings_table`
**Checklist**:
- The TES4 localized flag (`0x80`) — not "looks like a small integer" — decides literal vs
  `u32` string id. lstring id `0` means "no string" (empty, never the `<lstring 0x…>`
  placeholder, #4072); only a genuinely unresolved id yields the placeholder.
- `.STRINGS` has no length prefix; `.DLSTRINGS`/`.ILSTRINGS` do (`has_length_prefix`). A
  flag/data disagreement warns loudly (declared length vs NUL offset, ±1 band, #4178)
  instead of yielding shifted strings.
- Missing string files degrade to "no string" without panic or per-form warn spam; a
  localized plugin that resolves **zero** tables logs a diagnostic (#4073, `load_order.rs`).
- The language token is not the file-name segment: Skyrim spells it out
  (`dawnguard_english.strings`), FO4/FO76/Starfield use a short tag
  (`Fallout4_en.STRINGS`). `language_candidates` expands both; check it exists before
  reporting the defect (two audits re-derived it before it was filed, #4168).
- Decode is UTF-8-first with a cp1252 fallback and that order is load-bearing (#4170:
  English tables carry real cp1252 bytes, localized tables are real UTF-8). A flat cp1252
  decode is the regression; the fallback is not a bug.
- Loose files are found case-insensitively before the archive fallback
  (`load_with_archive`, #4176) — a case-mismatched mod override must beat the archive.
**Output**: `/tmp/audit/esm/dim_6.md`

### Dimension 7: `EsmIndex` → ECS Handoff & the Redux-Native Tier
Paths: `crates/plugin/src/{record,datastore,resolver,manifest,equip}.rs`, `esm/records/index.rs`; consumers `byroredux/src/cell_loader/references/`, `byroredux/src/npc_spawn.rs`
First step: `grep -rn 'resolve_entity_by_global_form_id\|find_by_form_id' byroredux/src | head -20`
**Checklist**:
- `EsmIndex` is a session-long bag of `HashMap<u32, _>`; nothing evicts it. Check growth on
  a large load order against `docs/engine/memory-budget.md` (RAM, not VRAM).
- Sample high-traffic `Option<u32>` FormID fields (`SCRI`, `RNAM`, `CNAM`, `PKID`, `XEZN`,
  teleport `XTEL`): the consumer must resolve via `resolve_entity_by_global_form_id`, not a
  raw `World::find_by_form_id`.
- `equip.rs` biped-slot constants: BMDT vs BOD2 vs FO4 bit meanings differ; each block cites
  its xEdit definition; slot → `addon_index` (`AddonData::addon_index`) matches
  *equipment_system*. `equip_template_tests.rs` covers template equip resolution.
- Redux-native tier: audit for **rot** (compiles against the current `Record` shape; docs
  consistent with `docs/engine/plugin-loading.md`) and dead-but-documented API. "Unused" is
  not a bug on its own; a missing ESH `0xFD` arm in `GlobalSlot` is a real gap.
**Output**: `/tmp/audit/esm/dim_7.md`

### Dimension 8: Real-Data Validation
Paths: `crates/plugin/examples/`, `crates/plugin/tests/parse_real_esm.rs`, `byroredux/src/{list_cells,sf_smoke}.rs`
First step: `cargo run --release -p byroredux-plugin --example esm_dim8_coverage -- <master.esm>` (one master at a time)
**Checklist**:
- Per `--game`: records seen / decoded / stubbed / skipped and *unknown sub-record codes
  per record type* (`esm_dim8_coverage` emits the TSV; `esm_dim8_bench` gives parse time
  + `EsmIndex` totals; other probes: `sf_smoke`, `probe_form`, `dump_record_subs`,
  `weather_coverage_census` — `ls crates/plugin/examples`). Unknown-subrecord share is the
  coverage signal per-game audits never compute across the whole file.
- A decode rate that drops between games is a schema-split suspect — correlate with Dim 4
  `GameKind` arms. Compare Starfield against the `--sf-smoke` resolve-rate baseline; a drop
  is a regression, not a new finding.
- Report parse **time** and peak RSS per master (ESM parse cost is on every cell load's
  critical path; NIF parse cost is `/audit-performance`).
- No windowed engine launch (*feedback_no_parallel_engine_launch*); example binaries and
  default `cargo test` only (see Phase 1 step 4 on `--ignored`).
**Output**: `/tmp/audit/esm/dim_8.md`

## Phase 3: Merge

1. Read all `/tmp/audit/esm/dim_*.md`; combine into `docs/audits/AUDIT_ESM_<TODAY>.md`:
   - **Executive Summary** — findings by severity; coverage posture; which games' real
     data was actually parsed.
   - **Record Coverage Matrix** — record type × game × {decoded, stubbed, skipped,
     unknown-subrecords}. The durable artifact; keep it even with no findings.
   - **Findings** — grouped by severity, deduplicated.
   - **Cross-Audit Pointers** — `/audit-<game>` Dim 1, `/audit-scripting` (VMAD),
     `/audit-character` (AVIF/class), `/audit-physics` (collision-relevant placement),
     `/audit-exterior`, `/audit-gameplay`, `/audit-parsers`.
2. Deduplicate against the per-game reports read in Phase 1 — a finding already filed under
   an `/audit-<game>` ESM dimension is `Existing: #NNN`, not new.

## Phase 4: Cleanup

`rm -rf /tmp/audit/esm`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_ESM_<TODAY>.md` (domain label `esm-plugin`; plus the
matching `game:*` when specific to one title's records).
