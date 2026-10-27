---
description: "Deep audit of the ESM/ESP plugin parser — GRUP walk, sub-record byte accounting, per-record schemas, FormID remap, CELL/WRLD walkers, ESM→ECS handoff"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# ESM / Plugin Parser Audit

Audit `crates/plugin/` (~80k LOC in `src/`) as a parser: GRUP walker, sub-record byte
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
`container.rs`, `condition.rs`, `weather.rs`, `weather_settings.rs` + `weather_settings_instance.rs`
(Starfield `WTHS` schema + values), `load_screen.rs`, `pkin.rs`, `misc/*.rs` incl. `story_manager.rs`,
`location.rs` (`LCTN`), `relationship.rs` (`RELA`), `challenge.rs` (FNV `CHAL`), `music.rs` (FO3/FNV `MUSC`),
`lmsw.rs` (Starfield `LMSW`/`MTPT` reflection blobs), `reflection.rs` (the BGS reflection container decoder shared by `WTHS`/`LMSW`/`MTPT`, moved out of `weather_settings.rs` by `ce5796431`, #6272; public paths unchanged via re-exports)),
`esm/cell/` (walkers `walkers.rs`, `support.rs`, `wrld.rs`, `helpers.rs`; `mod.rs` is a re-export hub over the
data tier `base_objects`/`cell_data`/`index`/`landscape`/`lighting`/`placed_ref`/`worldspace.rs`, `d7c47d741`),
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

**Suggested emphasis**: the 2026-08 → 09-11 reports yielded mostly Localized Strings,
ESM→ECS Handoff, CELL/WRLD and FormID remap; the 2026-09-21 report's MEDIUMs were Byte
Accounting (Oblivion `LVLO`/`LVLD`, FO4 `TERM`, `LTEX.GNAM`) and Header/GRUP (inflation
ceiling, skip-arm clamp), all found by Dim 8's real-data census — run it, don't skip it.
The 2026-09-29 report's MEDIUM/LOWs were again Byte Accounting + FormID: non-FormID `u32`s
fed to the remap (FO4 `TRDA` `FFFF` sentinel, Oblivion `EFID` char4 codes), fixed in
`9b099b31f` (#5075/#5076) — see Dim 3 shape (4). The 2026-10-08/10-09 MEDIUMs were decodes that
contradict their own `wbDefinitions*.pas` entry (SM `SNAM`/`RNAM`, REGN `WNAM`, PKIN `VNAM`,
SCOL/PKIN `FLTR`, `PKDD` Dialogue Type offset — all since fixed, #5493 `136048a95` / #5492 `dae08000d`) — open the xEdit definition for every decoder
touched since the last report rather than trusting its doc comment.

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
to it — `EsmReader::for_each_record`, the bounded walk the census/probe examples share since
`0b3ae0d13` (#5819), is not in it and is pinned only by
`reader::tests::for_each_record_filters_recurses_and_skips_unwanted`), `reader::tests::bounded_group_content_end_clamps_to_parent_end`, and the
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
- **Universal GRUP bound** (#3503/#3721/#4076/#4644): every self-recursive walker takes
  `bounded_group_content_end(header, depth, parent_end, name)` and threads `depth + 1`
  into its `_inner`; **skip arms** too seek to the parent-clamped end via `seek_to` (the
  depth-cap arm inside `bounded_group_content_end` included). The raw `group_content_end`
  may appear only in the non-recursive top-level loop (`records/parse.rs`, the only
  `skip_group` caller), `cell/wrld.rs::parse_wrld_group` and `grup_walker.rs::walk_info_records`'
  nested-group skip (both `.min(end)` themselves). Regression = a new recursive walker on
  the raw accessor, a dropped `depth`, or a recurse/skip site that loses the `parent_end`
  clamp (guards `overrunning_child_grup_in_a_skip_arm_seeks_to_parent_end`,
  `bounded_group_content_end_depth_cap_skip_is_parent_clamped` pin fixtures, not every site).
- `FLAG_COMPRESSED`: `data_size >= 4` checked before the subtraction; declared size is
  bounded by `record_inflation_ceiling` (`min(64 MiB, max(64 KiB, compressed×2048))`, from a
  seven-master census — worst real ratio 785.9:1, a Starfield `SFTR`, #4640; 512:1 rejected
  82 vanilla records and dropped the whole plugin) and the decoder is held to
  `take(declared + 1)`; mirrors `byroredux_bsa::safety::inflate_bounded` (#3410).
  Regression = either bound removed or the two implementations diverging.
**Output**: `/tmp/audit/esm/dim_1.md`

### Dimension 2: Sub-Record Byte Accounting (densest bug class)
Paths: `crates/plugin/src/esm/sub_reader.rs`, `esm/records/**/*.rs`, `esm/cell/support.rs`
First step: `git log --since=<last report> --name-only --format= -- crates/plugin/src/esm/records | sort -u` (decoders touched since the last report), then re-derive their widths from the cited xEdit range.
ESM analogue of `/audit-nif` Dim 1: a wrong field width shifts every later field and the
record still "parses" with wrong numbers.
**Guard**: `records::test_support::tests::no_module_redeclares_a_local_subrecord_builder`
(fixture hygiene only — nothing here guards decode widths; since #5454, `29f8fa2ef`, it flags
any fn returning `SubRecord` built from a parameter 4-CC, not only fixed names).
**Checklist**:
- Decoders read through `SubReader`; flag direct raw-slice indexing and any private
  cursor type. `take` / `wstring` / `counted` (a failed compound read leaves the reader where it
  started) carry the VMAD (`script_instance.rs`), `weather_settings_instance.rs` and NVNM
  (`misc/world.rs`) shapes since `e04caa7e8` (#6367) — the three per-decoder cursors and the
  `from_le_bytes(..try_into().unwrap())` sites in `reflection.rs` are gone (a width that disagrees
  with its type now declines instead of panicking); a new `Option`-returning cursor or
  `unwrap` on a slice-to-array in a decoder is the regression.
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
  differ); the four pre-FO4 WATR decoders share one head, `decode_legacy_head` driven by a
  per-game `HeadLayout` table (`626be5d12`, #5796; parity test
  `legacy_head_decodes_every_layout_through_one_pipeline`) — a per-decoder head fix is the
  regression; WATR `SNAM` is a remapped sound FormID (`sound_form`, `1dd217412`, #6045);
  WATR `decode_data_fo3nv` shared 0..28 prefix (wind speed/direction/amplitude/
  frequency at 0/4/8/12, direction degrees-on-the-wire converted once), pinned by
  `parse_watr_186_byte_record_reads_colors_at_40_44_48` and
  `wind_direction_converts_shipped_degrees_on_the_fo3nv_dnam_arm` (this field moved twice).
- WTHR (`records/weather.rs`) sizes are per game *and* form version: `PNAM` cloud colours step
  `cloud_color_slots(game, len)` TOD slots per layer (FO3/FNV 6 at ≥96 B, Creation-era 8 at
  1,024 B, else 4; `7f4c25e3e`, #6037); Skyrim `NAM0` is accepted only at the version-prefix
  sizes `skyrim_nam0_groups` maps (10/13/14/17 groups) and `DALC` also at 24 B (`67433ee3a`,
  #6038); cloud-layer FourCCs `00TX`..`O0TX` index `byte - '0'` via `skyrim_cloud_layer_index`
  for Skyrim **and** FO4/FO76/Starfield (`f16741886` #6040, `3ab1b5557` #6039 — FO3/FNV ignore
  them, `parse_wthr_00tx_fourccs_are_inert`); Oblivion `CNAM` is the lower layer 0 and `DATA`
  bytes 1-2 the layer speeds (#6041), and its cloud tints are `NAM0` group 2 (Clouds-Lower →
  `cloud_layer_colors[0]`) and group 9 (Clouds-Upper → `[1]`) — read as FO3/FNV's Unused until
  `7ed4c7dca` (#6258; `SKY_GROUP_2`/`SKY_GROUP_9`); FO3/FNV 24-byte `FNAM` carries fog power (#6042). The two arms (`parse_wthr`, `parse_wthr_skyrim`) share the RGBA unpack
  `sky_color_at`, the noon/midnight synthesis `synthesize_noon_midnight` and the fog head
  `read_fog_head` / `read_fog_power` (`ac9fa19a8`, part of #6223 — FO4/FO76/Starfield are deliberately **not** routed through the Skyrim decoder yet): a WTHR fix applied to one arm's copy of those is the regression.
- Strings: terminator consumed exactly once; a missing terminator cannot read past the
  sub-record.
- Leveled lists (`parse_leveled_list_for_game`): FO76 split `LVLO` (4 B ref) + float
  `LVLV`/`LVIV`/`LVCV` companions only under `GameKind::Fallout76`; the legacy row accepts
  ≥8 bytes (Oblivion omits Count → default 1) and `LVLD` bit `0x80` becomes `LVLF 0x01`, not
  chance-none 128 (#4638). A sub-8-byte legacy `LVLO` must never become a valid reference.
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
against, a raw plugin-local id. **Inverse shape (4)** — a non-FormID `u32` routed *into*
`remap_fid` (only `0` short-circuits, so every row warns through the out-of-range arm on a
plugin with masters): xEdit `wbFormIDCk([…, FFFF])` fields go through
`remap_fid_or_sentinel`, and Oblivion `EFID` (a `wbChar4` effect code resolved via
`magic_effects_by_code`) through the game-gated `remap_efid` (`records/common.rs`;
`9b099b31f`, #5075/#5076); PKIN `VNAM` is xEdit's integer *Version*, stored raw as
`PkinRecord.version` (`0d436e642`, #5422, pinned by `pkin_vnam_version_stays_raw_under_remap`).
A new sentinel-bearing field, char4 code or plain integer calling `remap_fid` inline is the
regression. SCOL/PKIN `FLTR` is a `wbString` filter path: both parsers store `filter: String`
via `read_zstring` with no `remap_fid` (fixed in `136048a95`, #5493; pinned by
`pkin_fltr_filter_path_stays_raw_under_remap` + `parse_scol_fltr_decodes_the_filter_path`).
CTDA `param_1` is remapped only for functions whose first parameter is a FormID:
`condition.rs::param1_is_form_id` mirrors the scripting catalog by raw function index (rows
for fn 56/71/74 were missing until `722c06a76`, #5637; M47.2 added the location/keyword/
relationship/quest-variable rows); `param2_is_form_id` is the second-parameter twin (fn 605/181/
576/606/608 — everywhere else `param_2` is a literal); the drift guard lives in the scripting
crate (`param1_form_id_table_matches_the_condition_catalog`) — a new catalogued
FormID-param function needs its row.
**Checklist**:
- `GlobalSlot::Regular` keeps 24 bits and regular allocation stops at `0xFB` — `0xFC` is the
  Papyrus VM's synthetic alias-owner byte (`0edc61bbf`, #5988; a regular `0xFC` slot is the
  regression); `GlobalSlot::Light` (`0xFE` space) packs a 12-bit
  sub-index and keeps only the low **12** object-id bits (`0x0FFF` masks on both sides);
  `GlobalSlot::Medium` (`0xFD`, Starfield) packs an 8-bit sub-index + 16-bit object id.
  The TES4 master bits are per game (#4639, `read_file_header`): Starfield small `0x100` /
  medium `0x400`, Skyrim SE + FO4 light `0x200`, every other game none — a `0x200` test on
  Starfield (*Update*) or a pre-ESL game is the regression. `allocate_global_slot`
  (`load_order.rs`) must keep the three spaces disjoint.
- `FormIdRemap::remap` arms (`FormIdScheme::Positional`, every game but Starfield): raw `0` → `0`
  (null sentinel, must not become a live ESL id);
  self-reference (`mod_index == master_slots.len()`); in-range master; standalone with no
  masters (pass through, `debug` — the vanilla Oblivion `0x01` artifact, #1308; must NOT be
  "fixed" into a clamp — two forms would collide in `EsmIndex`); out-of-range with masters
  (pass through, `warn`). Starfield plugins get `FormIdScheme::Starfield` (chosen per plugin
  from its detected game in `load_order.rs`; `0a2955d33`, #6118): `0xFE` + 12-bit ordinal
  among the file's **small** masters, `0xFD` + 8-bit ordinal among its **medium** masters, any
  other top byte an ordinal among its **regular** masters only, ordinal == that kind's count
  = self, an unlisted master passed through with a `warn` (`remap_starfield`; positional
  decode had two small masters claiming 61k shared ids). Pinned by the `starfield_*` tests in
  `reader.rs`; a fixture or new caller building `FormIdRemap` for Starfield with the default
  scheme is the regression.
- Master list read in file order; a missing master fails diagnosably, not as a silently
  shifted slot table.
- Multi-plugin is live (`--master` repeatable → `parse_esm_with_load_order`); `parse_esm`
  passes `None`. Verify every embedded FormID in a newly-touched parser goes through
  `remap_fid`. The guard walk sees only `pub`/`pub(crate)`/`pub(super) fn parse_*` in
  `records/` (files named `tests.rs` and `#[cfg(test)]` modules excised) — `decode_*` /
  `from_subs*` helpers and `cell/` are outside it.
- Parallel walk (#3813, `parse_record_indexes_in_load_order_with_archive`): walks run on
  the rayon pool, but slot allocation, remap construction, `.STRINGS` loads and the
  `merge_from` fold stay on the calling thread in load order — fold order, never completion
  order. Guard: `load_order_parallel_tests::parallel_walk_matches_the_inline_walk_on_a_localized_master_chain`.
**Output**: `/tmp/audit/esm/dim_3.md`

### Dimension 4: Record Schema Dispatch & Coverage
Paths: `crates/plugin/src/esm/records/{parse,index,items,container,actor_value_derive,load_screen}.rs`, `records/dispatch_*.rs`, `records/misc/*.rs`, `crates/plugin/src/equip.rs`
First step: `cargo test -p byroredux-plugin -- dispatch_handled_fourccs every_index_map_is_a_category`
**Guards**: `records::tests::dispatch_handled_fourccs_matches_the_live_dispatch_arms`
(`DISPATCH_HANDLED_FOURCCS` ↔ the live match in `parse.rs`; era-gated arms count as
handled, `PDCL` is deliberately absent; `LCTN`/`LCRT`/`RELA` are routed through `dispatch_misc_stub` into typed maps since M47.2 — `parse.rs`'s `DISPATCH_HANDLED_FOURCCS` doc still says LCTN has no arm, that prose is stale);
`index::tests::every_index_map_is_a_category_or_a_recorded_exclusion` (every `EsmIndex`
map is a `categories()` row — `map_category!` or a hand-written row recognized by its count
closure, like `dialogues` and `game_settings` — or a reasoned exclusion: `record_types`, the derived
`generic_greeting_form` cache, the derived `game_settings_by_edid` side-table; the non-`HashMap` `deleted_record_metadata` /
`skipped_unconsumed_groups` sit in its `MANUALLY_MERGED` table); `cell::plugin_loading_doc_pin_tests::the_documented_esm_cell_index_lists_every_field` and
`…::the_audio_record_split_matches_the_dispatcher` (docs ↔ code).
**Checklist**:
- Build the live matrix per record type: typed decode / minimal stub
  (`parse_minimal_esm_record`, EDID+FULL — `dispatch_misc_stub.rs` also hosts typed
  decoders: `LSCR`, `CLOT`/`CCRD`/`CMNY` → `items`) / skipped. Report the delta against
  *record_type_catalog* as coverage, not bugs. A stub must advance the reader correctly;
  the unknown-record path skips by declared size and never `warn`s per record on a vanilla
  master.
- Per-game schema splits within one record (`ARMO/WEAP/AMMO DATA`, `BOD2` vs `BMDT` vs
  Skyrim's pre-BOD2 `BODT` — 8 or 12 B, same leading biped mask, read in both `ARMO` and
  `ARMA` since `479414ffe` (#5358; every Skyrim.esm ARMA authors it, so BOD2-only decoding
  zeroed all 766 masks), `SCOL/PKIN/TXST`, FO76 `LVLO`) switch on `GameKind`. Oblivion's 4-byte
  `ARMO BMDT` is a u16 Biped Flags + General Flags byte — the mask is the u16 only
  (`a8e37e5fd`, #5696; a u32 read put General Flags in bits 16-23 on 661 of 996 armors);
  FO3/FNV keep the u32. **`ACRE` is not Oblivion-only**
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
  the per-field walk instead of comparing only the chain ends. "Use Model/Animation"
  (`TEMPLATE_FLAG_USE_MODEL_ANIMATION`, `ResolvedNpc::model`) resolves model-less templated
  `CREA` shells through their `LVLC` pick (`011ad0c07`, #6075; 63% of FNV fauna placements).
- `EsmIndex::merge_from`: last-write-wins matches Bethesda override semantics; `game` is
  only adopted when `other.total() > 0` (a failed-parse `EsmIndex::default()` must not
  relabel the load order as FO3/FNV, #3403); `character_rules` is first-non-`NONE` wins.
  **Exception — `dialogues`** (`168b81322`, #5375): an override DIAL replaces only the
  header fields and folds its INFOs into the master's list by FormID (same id in place, new
  id after its `PNAM` predecessor), and the prune arm drops Deleted-INFO tombstones from every
  topic; whole-record `extend` erased 8.5–9.9k FNV INFOs per DLC. An override's `PNAM == 0`
  run keeps authored file order through one per-override contribution cursor
  (`fold_infos_into_topic`, `5356c8780`, #5494 — the first contribution goes to the front, later
  ones follow the previous; the run's position relative to the master's heads is an unsourced
  engine choice). Since `0c2bd4d6d` (#5644) the fold is one pass over a FormID → node map;
  `fold_info_into_topic_reference` is the test-only one-at-a-time twin and
  `batched_info_fold_matches_the_one_at_a_time_fold` pins that the two agree.
  `game_settings` is also a hand-written category row: GMST resolves by EditorID in load order, not FormID
  luck — `insert_game_setting` (the only parse-path insert) keeps `game_settings_by_edid`
  (lower-cased EditorID → winning FormID) coherent, `game_setting_float` reads it first, and the
  prune arm drops pointers into tombstoned FormIDs (`824bb58c6`, #6350; pinned by
  `game_setting_duplicate_editor_id_across_distinct_form_ids_resolves_last_loaded`). A parse path
  inserting into `game_settings` directly leaves the linear-scan fallback (test fixtures only) serving.
  Top-level maps merge through `categories()`; `EsmCellIndex::merge_from` destructures
  exhaustively (#4904 — a `..` there is the regression that dropped `landscape_grasses`).
  A CELL override's record fields come from the override **as parsed** — an omitted
  sub-record is omitted on purpose (Update.esm drops `XCWT`, a DLC drops `XCRI`); only child
  records (placements, landscape, navmeshes, pathgrids) and a missing EDID inherit from the
  base (`merge_cell_override`, exhaustive `CellData` destructure; `2fa227d72`, #6086, pinned by
  `cell_record_fields_come_from_the_winning_override` / `child_records_still_inherit_from_the_base`).
  A REFR an override re-authors under another CELL is swept from every other held cell
  (`765e387a0`, #6084, `a_refr_moved_into_another_cell_leaves_one_copy`).
- M47.2 added typed maps: `locations` (`LCTN`, `misc/location.rs`), `location_ref_types` (`LCRT`),
  `relationships` (`RELA`, `misc/relationship.rs`), `condition_forms` (Starfield `CNDF`),
  `keywords` (`KYWD` EDID only), `SNDR` into the `sounds` map via `parse_sndr` (`soun.rs`),
  RACE FO4 `SRAC`/`SADD`/`SAKD`/`SGNM`/`SAPT`/`STKD`/`SRAF` subgraph data, `KWDA` on
  `CommonNamedFields`, PERK entry bodies (`PerkEntry`/`PerkEntryBody`, EPFT/EPFD per the
  xEdit layout), and a "Use Base Data" template chain terminal (`ResolvedNpc::base_data`). The
  `EsmIndex::categories()` count is pinned in `records::tests` (111 at HEAD) — read the live
  number there; each new map must be a category row or a recorded exclusion.
- `actor_value_derive.rs` is the CHARAL feed — formulas belong to `/audit-character`.
- Settled decodes — verify they hold, do not re-derive: `INAM` is one FormID array (#3356);
  `CREA CNAM` is not a class (#3383); `FACT` rank ladder (#3338); `ARMO` contributes every
  race-matching `ARMA` (#3357), `MOD3` is the female mesh (#3414); a `REFR` tombstone
  removes the placement wherever it lives (#3362); FO76 `HEDR` is patch-dependent — 279.0 on the 2026-09-20 patch, never pin it (#3405, #4643);
  DIAL quest ownership reads `QSTI` **or** `QNAM` (Skyrim authors `QNAM`; `parse_dial`); FO3/FNV/Oblivion INFOs also carry their **own** owning quest in `InfoRecord.quest` (`QSTI`, remapped, 23,247/23,247 FNV INFOs; `dbc07e8f0`, #5271 — Skyrim+/FO4 author none, `0` = use the topic's ownership); INFO `DATA` is typed `InfoDataHeader` (info_type / next_speaker / flags1 / flags2; Goodbye = Flags 1 bit 0) and Skyrim's 8-byte `DATA` lands raw in `skyrim_data` (`25b678106`, #5295); `SMBN`/`SMEN`/`SMQN` decode into `story_manager_nodes` (`misc/story_manager.rs`, #5366 closed — the SM dispatches live): `SNAM` is the **previous** sibling (xEdit *Previous Node*; `846e4a6dd`, #5385 — the same-parent census holds in both directions and cannot settle it), `SMQN` `RNAM` is stored hours × 24 and decoded ÷ 24 (xEdit scale `1/24`; `a621d5352`, #5386 — node `HNAM` stays raw), and `XNAM`/`MNAM`/`HNAM`/`QNAM`/`FNAM`/`UNAM` + `DNAM` `0x2`/`0x40000` are named from xEdit (`5489cbe65`, #5420; per-quest `FNAM`/`UNAM` live on `SmQuestLink` beside the `NNAM`/`RNAM` they follow — a node-level field let each later quest overwrite the last, `0edd1dede`, #5515); INFO Flags 1 bit 5 = Random End (`random_end`, `0436bea5c`, #5397); REGN `WNAM` is the owning **worldspace** (`worldspace_form`, remapped), never a weather, and REGN `CNAM` is xEdit-undefined with zero vanilla occurrences (`bd052048a`, #5421); `NPC_.WNAM` is the per-NPC skin ARMO (`worn_skin`, remapped, no game gate — pre-Skyrim ships none; `edb5fbdfe`, #5359); Oblivion `RACE`'s body-section `ICON` run (INDX restarts at 0 past `NAM1`) lands in `body_part_textures` (`1b45763fa`, #5487); Starfield `WTHS` decodes the REFL/RDIF reflection schema (`weather_settings.rs`, reflection container in `reflection.rs`, #5364; the DIFF walk is depth-capped at `MAX_NESTING_DEPTH` 64, `02187c7f9`, #5687, pinned by `hostile_self_referential_diffs_decline_at_the_depth_cap`) and, since `2c203278f` (#5627), the instance values (`weather_settings_instance.rs`: root `OBJT` value tree + `DIFF` overrides with `LIST`/`MAPC`/`USER`/`USRD` side chunks; grammar grounded from `Starfield.esm` itself — any walk/schema disagreement declines the whole instance to the census-only state, never guesses); `EsmIndex::synthesize_wths_weathers` runs on the merged index after the load-order fold (`load_order.rs`) and lifts only the grounded fields (day/night imagespace, `Colors.Sun`/`Sunlight`/`FogNear`) into a `WeatherRecord` under the WTHS FormID in `weathers` — fog distances, wind and transition floats stay unlifted (real-data 181/181 census in `parse_real_esm.rs`, ignored lane); `DIFF` is (u16 field index, inline value) pairs to the `0xFFFF` terminator and `USRD`/`MAPC` join the instance census (`f0f8dceef`, #5514; real-data pins 163 in-range indices / 47 `USRD`); the CDB builtin ids live in one table (`1f56d5742`, #5615); FO76 `LSCR` decodes on the modern (FO4-shape) arm incl. `BNAM` → icon (`47f927cbf`, #5345); FO3/FNV `PKDD` Dialogue Type is the u32 at offset 16, and Oblivion `PKDT` (4 B) / `PTDT` (12 B) legacy shapes are accepted (`dae08000d`, #5492/#5504); FO3/FNV non-auto-calc NPCs keep their authored `DATA` SPECIAL in `NpcRecord::data_attributes` (`4957536ba`, #5338); FO4 `NPC_` template flags resolve per flag through `template_actors` (`TPTA`, 13 entries in xEdit `wbTemplateFlags` order, `0` = use `TPLT`; `99c22a8db`, #5498); `MSWP` `CNAM` is the Color Remapping Index palette row (`color_remap_index`, `0b2ffed89`, #5497); `MGEF` `DATA` offsets/flags are named constants in `misc/magic.rs`'s `mgef_data` (`abe8d7e96`, #5614); Starfield `NPC_` `ACBS` is 16 B (the FO4 layout minus the trailing words) and shares the FO4 arm under a `>= 16` gate (`78facba89`, #6120 — no arm matched, every Starfield NPC was male with no template flags); ACBS Essential/Protected decode in `parse_npc` (`NpcRecord.essential`/`.protected`; bit 11 is Protected from Skyrim on, No Blood Spray on FO3/FNV; `3162b21b8`, #5921); the ACBS arm is `Fallout4 | Fallout76` at `>= 20` B (FO76 ships the FO4 layout: all 6,551 `SeventySix.esm` NPC_ ACBS are 20 B, `c4884977a`, #6240); the `RACE` `ANAM` skeleton run is read for Skyrim/FO4/FO76/Starfield (`e8e00326d`, #6122; FO76 `4a7963156`, #6297 — 307 runs over 157 RACEs); Oblivion `RACE` body-section tail `MODL` lands in `RaceRecord::tail_models` (male, female; only Argonian/Khajiit author one, `a6ab3625e`, #5753); Starfield bleedout decodes into `RaceRecord::bleedout` (`DAT2` +160) and `NpcRecord::bleedout_override` (`ONA2` flag 5) — decode only, no recovery rule is wired (`a1b72625f`, part of #6259); `PACK` `PSDT` Day-of-Week decodes into `PackDays` (−1 any, 0–6, 7 weekdays, 8 weekends, 9 MWF, 10 TT; `3ce1e1e3a`, #6079 — consumed for FO3/FNV, and Oblivion since `5d7f08e56`, #6257, which also decodes PSDT byte 2 into `PackSchedule::date`); the 12-byte Skyrim+ PSDT duration is **minutes** (`duration_minutes` + `start_minute`; the 8-byte FO3/FNV/Oblivion hours are ×60 at decode, `7e2f21482`, #6210 — an hours reading made 1,536 of 1,545 timed Skyrim windows always-on); `PKW3`/`PTD2` (FO3/FNV Use Weapon data and second target, `PackUseWeapon`, `10f8cbb91`); Skyrim/FO4 `FACT` Crime tab (`parse_fact_crime` → `FactionRecord::crime`, `4cfcee4b9`) and `CLAS` `DATA` flags on Oblivion/FO3/FNV (`ClassRecord::class_flags`, Guard bit 1, `1a67c9f30`); `CHAL` decodes only under `Fallout3NV` — Starfield's 8 `CHAL` have no `DATA`/`SNAM` (`f90aea105`, #6298); `MSWP` `FNAM` is the CK *Tree Folder* (`tree_folder`, tooling only), never a path filter (`f81e6e722`, #5756); FO3/FNV `WEAP` `DNAM` carries Base VATS To-Hit at offset 28 (`vats_base_to_hit`) and the standard AP cost at offset 68 (`ap_cost`), while the VATS sub-record's offset-12 float is the Override-tab AP, kept as `WeaponVats::ap_override` (`ff97cdd93`, part of #6350 — the #3324 reading of it as *the* AP cost was a mislabel); `BPTD` decodes the full part run (`BPTN`/`BPNN`/`BPNT`/`BPNI` strings + the 84-byte `BPND`, `BptdRecord::parts`) with short payloads defaulting, `CREA` links its record through a standalone `PNAM` (`NpcRecord::body_part_form`; RACE carries none) and humanoids resolve the reserved `PLAYER_BODY_PART_DATA` `0x1C` / `DEFAULT_BODY_PART_DATA` `0x1D` slots (`aebcc99a1`, part of #6350); FO3/FNV `RACE` authors the same body-section `ICON` run under `NAM1` with a three-slot vocabulary (0 upper body, 1/2 hands) — `body_part_textures` is no longer Oblivion-only (`9d1a262ed`, #6090; Skyrim+ authors none); `SOUN` `SNDD`/`SNDX` bytes 0/1 carry min/max attenuation (`SounRecord::attenuation_units`; min ×5, max ×100 game units) plus the Menu Sound / 2D bits (`41b53ab41`, #6281); the CRVA, PACK (`PACK_LOCATION_LEN`), ACBS and Oblivion-CTDA length gates are named constants (`64f26db8c`, #6393).
- `equip.rs::main_body_bit` FO76/Starfield arms are a **provisional inference**
  (`PROVISIONAL (#4074)`, pinned by `fo76_and_starfield_arms_are_marked_provisional`) —
  known and marked; report only if the marker is removed without an xEdit citation.
**Output**: `/tmp/audit/esm/dim_4.md`

### Dimension 5: CELL / WRLD Walkers & Placement Data
Paths: `crates/plugin/src/esm/cell/**/*.rs`, `esm/records/misc/world.rs`, `esm/records/spatial_units.rs`
First step: `git log --since=<last report> --format='%h %cs %s' -- crates/plugin/src/esm/cell crates/plugin/src/esm/records/misc/world.rs`
**Checklist**:
- Cell child groups: type 6 is the `Cell Children` *container*; 8 = Persistent, 9 =
  Temporary, 10 = Visible Distant nest inside it (#4169 — earlier legends said 6/8/9).
  `PlacedRef.group_type` is re-derived at each nested GRUP in `parse_refr_group_inner`
  (only the nested position accepts 10; at CELL/WRLD level 10 is Quest Children). No
  consumer reads it yet — verify it stays populated per-scope, not that a consumer exists.
- Interior and exterior CELL sub-record walkers share one decode, `CellSubrecordFields::absorb` (`cell/helpers.rs`, #5309), and one `CellData` build, `CellSubrecordFields::into_cell` (`d6d216b91`, #5815) — a field added to only one walker, or a hand-built `CellData` literal back in `walkers.rs`/`wrld.rs`, is the regression. All three walkers depth-bounded (see Dim 1): `parse_cell_group`, `parse_wrld_children`,
  `parse_refr_group`.
- Placement header flags (`f87490826`, #4813/#4814): `PlacedRef.initially_disabled` =
  `0x800` on every placement type; `PlacedRef.starts_dead` = `0x200` on `ACHR` gated on
  `EsmVariant::Tes5Plus`; `starts_unconscious` = `0x2000` on `ACHR`, FO4/FO76/Starfield only
  (#5017). The placement bit is **not** the corpse marker on every game, and each other game
  is settled at its own layer: Oblivion = base `NPC_`/`CREA` header `0x80000`
  (`base_actor_starts_dead`, `dispatch_actor.rs`, Oblivion-gated, #5013); FO3/FNV = base
  `DATA` health ≤ 0 (`NpcRecord::starts_dead`, #5005 — `0x80000` is not a marker there,
  and `XRGD` is a ragdoll *pose*, not a death signal) plus the script-killed idiom
  (`717f39a82`, #5223 — a dismember-family trigger's `XLKR` target; FO3/FNV `XLKR` is **4 B**,
  the target FormID alone, vs TES5's 8 B keyword+target — a `len >= 8` gate silently dropped
  every FO3/FNV link). A zero placement `starts_dead` means "no placement-level marker",
  not "alive"; the cell loader ORs both
  (`byroredux/src/cell_loader/references/mod.rs` → `reference_state.rs::apply_starts_dead`). Flag any per-game gate that reads one of these bits on a game
  whose xEdit list does not define it.
- Starfield metric → 70/m lift (`records/spatial_units.rs::normalize`, called once at the
  end of each plugin's walk in `parse.rs`): positions, light radii, XCLL/LGTM fog/fade +
  SF height-fog (#5002; the 20-field lighting tail decodes once in
  `cell::decode_starfield_lighting_tail` and the height-fog lift is the shared
  `starfield_heights`, `9dbd59b8b`, #5346), WTHR fog distances (only when
  `fog_distances_authored` — an FNAM-less weather keeps the unlifted 10 000 default, `ec75b1439`,
  #5301, pinned by `starfield_wthr_without_fnam_keeps_the_unlifted_fog_defaults`) + height-fog tail (#5001/#5134), water heights,
  WATR `DNAM` lengths ×70 and per-metre absorption ÷70 — only for offsets the record
  authored (#5151, `24cb577d2`), LAND heights, NAVM vertices, XRGD ragdoll-pose offsets (`5f1a862be`, #5299; Euler stays wire-valued). FO76 is BU-native except the
  shared-decoder WATR absorption lane, its only lift (#5169). Three Starfield WATR `DNAM` lane
  families (noise-UV tiles, displacement simulator, normal falloff) are deliberately **not**
  lifted — unit unsettled, pinned by `starfield_watr_defers_the_unclassified_dnam_lanes`; the
  LGTM + cell-XCLL lift is pinned through `parse_esm` by
  `starfield_index_lifts_lgtm_and_xcll_lighting_distances` (`2dab4ff48`, #5170/#5171,
  `records/spatial_units_tests.rs`). Check new Starfield distance
  fields join it, dimensionless fields (XSCL, Euler) stay out, decoder defaults for an
  unauthored field are never lifted, and no path normalizes an index twice.
- M47.2 placement/base fields, all remapped: REFR `XAPR` (activate parent + delay, `XAPD`
  Parent Activate Only), `XMRK`→`FNAM` map-marker flags, per-reference `XEZN`
  (`PlacedRef.encounter_zone_form`); base-object `DEST`/`DSTD` → `DestructionData`. `ECZN DATA`
  is per game (`parse_eczn(.., game, ..)`: FO3/FNV 8 B, Skyrim/FO4/Starfield add the `LCTN` and
  max level; xEdit `wbECZN`). `EsmCellIndex::cell_by_form_id` is a lazily-built O(1) lookup
  (`cell_form_id_lookup`, #5489), and `locate_refr` / `cell_for_refr_form_id` answer from the
  twin `refr_lookup` reverse index (`56fc348b8`, #6125; first cell holding a FormID wins, in
  interior → exterior → persistent order); `merge_from` resets both — a CELL-map edit after
  the first call leaves them stale. Starfield REFR `XLMS` is a remapped list of `LMSW` FormIDs
  (`PlacedRef.layered_material_swap_refs`); `layered_material_swap_entries` flattens it
  (collection members recursively, then own entries; cycle- and depth-capped) over the
  `layered_material_swaps` / `material_path_forms` maps (`17bd40f4f`, #5757).
- Lighting-template inheritance is per-field; "absent" and "authored zero" stay distinct.
- CELL `XCCM` is per-game (`3b28dca71`, #5463): TES4/FO3/FNV = Climate (CLMT) →
  `CellData.climate_override`; TES5/FO4 = "Sky/Weather from Region" (REGN) → `sky_region`
  (`xccm_targets_region`, `cell/helpers.rs`; decoded, no consumer yet; FO76/Starfield keep the
  CLMT reading, no verified xEdit definition). Guards `parse_cell_oblivion_fo3nv_xccm_populates_climate_override`,
  `parse_cell_tes5_fo4_xccm_is_a_region_not_a_climate`. Regression = a REGN id reaching `climate_override`.
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
  `INT_MIN` / `FLT_MAX` sentinel → suppress (`gated_water_height` in `esm/cell/helpers.rs` +
  `water_height_is_explicit`; `docs/engine/watal.md`).
- Exterior grid `XCLC`, worldspace parenting and selective-inheritance flags (`wrld.rs`):
  a child inherits only the flagged categories. Oblivion authors no `PNAM`, so
  `parse_wrld_group` stamps land/LOD/water/climate inherit bits on every Oblivion child
  (`67afa4b95`, #5374 — pre-FO3 "child ⟹ inherit all"; the flag-less duplicate fallback was
  deleted in `65cadde2f`, #5516, so the stamp is the only copy — pinned by
  `oblivion_child_gets_the_inherit_all_stamp_and_eras_stay_real`); an authored zero `PNAM` on
  FO3+ must stay zero. After the merge, `EsmCellIndex::inherit_parent_landscapes` (gated to Oblivion and
  FO3/FNV at its caller — `apply_exterior_processing` / `needs_exterior_processing` in `byroredux/src/cell_loader/load_order.rs`, moved
  into the shared parse by `588c87bc4`, #6123; FO3's four "Use Land Data" children measured co-located with the Wasteland's LAND, `a9472103b`, #6089 — the real-data census fills 100–120 LAND-less cells; Skyrim+ stays out) fills a child's LAND-less cells from the nearest
  ancestor's LAND (`75b921cf4`, #5505). `XCLC` byte 8 is kept raw as `CellData.land_flags`
  (Hide Quad bits 0-3, FO4+ No Collision bits 4-7; `79477b4a3`, #6043) — like `grid`, a CELL
  record field, so an override's value wins as parsed (#6086, Dim 4). An EDID-less `WRLD` clears the current-worldspace binding so its children are not
  filed under the previous worldspace (`6f803356d`, #6047).
- `parse_land_record`: quadrant/layer counts and splat rows are fixed-stride. An `ATXT`
  with no following `VTXT` must flush with `alpha: None` (`pending_atxt`, #4078;
  measured 14× on `Oblivion.esm`). `LTEX.GNAM` is an **array** of grasses, kept in
  authored order into `landscape_grasses` by `parse_ltex_group` (#4642; last-wins was the bug).
  The `LTEX.TNAM` → `TXST` join is re-run against the merged load order
  (`landscape_texture_txsts` + `resolve_landscape_texture_sets`, `65add6a2e`, #6036) — a
  per-plugin-only join loses every DLC LTEX naming a master TXST. LTEX Havok data lands in
  `landscape_texture_havok` (`LandscapeTextureHavok`; the walk's out-maps are one
  `cell/support.rs::LtexTables` sink): Oblivion/FO3/FNV `HNAM` = u8 material + friction +
  restitution, Skyrim/FO4 `MNAM` = remapped `MATT` FormID with `HNAM` friction + restitution
  only (`c53d87679`, part of #6048; friction/restitution stored as authored, unapplied).
- FO4 precombine CELL sub-records (`CellSubrecordFields::absorb`): the `XCRI` tail is
  `ref_count / 2` × (ref FormID, bake hash) pairs → `CellData.absorbed_ref_bakes` (the skip
  set); `XPRI` is the disjoint previs list, validated and **not** stored (`777351ec1`,
  #5484 — the two were inverted, double-drawing every baked STAT/SCOL). Guards
  `parse_cell_xcri_pairs_remap_and_keep_their_bake_hash`,
  `parse_wrld_exterior_cell_captures_precombined_xcri_xpri`.
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
- Tables and the localized flag are thread-locals: since the parallel walk (#3813) the
  caller only loads tables (`load_string_tables`) and each walk installs its own
  `StringsTableGuard` on the walking thread. A guard installed on the caller yields
  `<lstring 0x…>` placeholders under the pool.
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
  raw `World::find_by_form_id`. That resolver answers from a lazily rebuilt local-id → entity index
  owned by `FormIdPool` (`FormIdPool::entity_with_local`, keyed on the `FormIdComponent` structural
  generation plus pool length; first entity in storage order wins on a duplicate id; `ab3a604bf`,
  #6338) — a per-call scan of every `FormIdComponent` is the regression (per-actor-per-frame Flee/UseWeapon callers).
- `equip.rs` biped-slot constants: BMDT vs BOD2 vs FO4 bit meanings differ; each block cites
  its xEdit definition; slot → `addon_index` (`AddonData::addon_index`) matches
  *equipment_system*. `equip_template_tests.rs` covers template equip resolution.
- Redux-native tier: audit for **rot** (compiles against the current `Record` shape; docs
  consistent with `docs/engine/plugin-loading.md`) and dead-but-documented API. "Unused" is
  not a bug on its own — `DataStore` + `DependencyResolver` + `PluginManifest` have no
  production consumer and are kept as design surface: #5609 (`490e76397`) put status banners on
  `lib.rs`, `plugin-loading.md` and `architecture.md` and a ROADMAP row. A doc presenting them as
  the live load path again is the regression; the absence of a consumer is not a finding.
  Likewise seven typed maps (ENCH, REPU, EXPL, CSTY, IPCT, IPDS, COBJ) are parsed with no
  runtime reader and annotated as such in `index.rs` + `esm-records.md` (`934385de1`, #6180).
  `body_parts` (BPTD) left that set when the VATS targeting scan began reading it
  (`byroredux/src/systems/vats.rs`, #6350) — its `index.rs` doc still carries the "no runtime
  consumer yet" annotation, which is now the stale side.
**Output**: `/tmp/audit/esm/dim_7.md`

### Dimension 8: Real-Data Validation
Paths: `crates/plugin/examples/`, `crates/plugin/tests/parse_real_esm.rs`, `byroredux/src/{list_cells,sf_smoke}.rs`
First step: `cargo run --release -p byroredux-plugin --example esm_dim8_coverage -- <master.esm>` (one master at a time)
**Checklist**:
- Per `--game`: records seen / decoded / stubbed / skipped and *unknown sub-record codes
  per record type* (`esm_dim8_coverage` emits the TSV; `esm_dim8_bench` gives parse time
  + `EsmIndex` totals; other probes: `sf_smoke`, `probe_form`, `dump_record_subs`,
  `weather_coverage_census` — `ls crates/plugin/examples`; one-liner probes are `inspect.rs`
  subcommands since `b997ea133`, #5856). Unknown-subrecord share is the
  coverage signal per-game audits never compute across the whole file.
- A decode rate that drops between games is a schema-split suspect — correlate with Dim 4
  `GameKind` arms. Compare Starfield against the `sf_smoke --tsv` baselines in `.claude/audit-baselines/sf-esm/`
  (regenerated `6cbae341c`, #5758; `sf_esm_baseline_handled_column_tracks_the_dispatch_list` pins
  their handled column to the dispatch list); a drop
  is a regression, not a new finding.
- Report parse **time** and peak RSS per master (ESM parse cost is on every session's
  boot path; door transitions reuse the resident index through `SharedParseCache` in
  `load_order.rs` unless the plugin set or a file's length/mtime changed, `588c87bc4`, #6123;
  NIF parse cost is `/audit-performance`).
- Real-data tests resolve installs through `esm/test_paths.rs` (`*_data_dir()` + `require_or_skip`);
  `BYROREDUX_REQUIRE_GAME_DATA` turns an absent install into a failure on the strict lane (`3e4bfab51`, #6184) — a
  test that re-inlines a Steam root or returns `ok` on a missing master is the regression.
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
