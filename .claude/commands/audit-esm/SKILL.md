---
description: "Deep audit of the ESM/ESP plugin parser — GRUP walk, sub-record byte accounting, per-record schemas, FormID remap, CELL/WRLD walkers, ESM→ECS handoff"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# ESM / Plugin Parser Audit

Audit `crates/plugin/` — the second-largest crate in the workspace (~45k LOC,
95 files) and, until this skill landed, the **largest subsystem with no owner
audit**. Per-game audits (`/audit-fnv`, `/audit-skyrim`, …) each sample one
game's slice of it; nothing has ever audited the parser as a parser: the GRUP
walker, sub-record byte accounting, per-record schema dispatch, the FormID
load-order remap, the CELL/WRLD walkers, and the `EsmIndex` → ECS handoff.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crate**: `crates/plugin/src/`

- `crates/plugin/src/esm/reader.rs` — `EsmReader`, `EsmVariant`, `GameKind`,
  `FormIdRemap`, `GlobalSlot`, `RecordHeader`, `GroupHeader`, `SubRecord`,
  `FileHeader`; zlib record decompression.
- `crates/plugin/src/esm/sub_reader.rs` — `SubReader`, the bounds-checked
  cursor every per-record decoder is supposed to go through.
- `crates/plugin/src/esm/records/` — the record layer: `mod.rs` (`parse_esm`,
  `parse_esm_with_load_order`, GRUP label dispatch), `index.rs` (`EsmIndex`),
  the eight `dispatch_*.rs` group routers, and the per-type decoders
  (`actor/`, `items.rs`, `container.rs`, `condition.rs`, `script.rs`,
  `script_instance.rs`, `weather.rs`, `climate.rs`, `tree.rs`, `global.rs`,
  `mswp.rs`, `movs.rs`, `pkin.rs`, `scol.rs`, `outfit.rs`, `list_record.rs`,
  `grup_walker.rs`, `actor_value_derive.rs`, `common.rs`, and
  `misc/{character, dialogue, effects, equipment, imagespace, magic, pack,
  quest, scene, water, world}.rs`).
- `crates/plugin/src/esm/cell/` — `mod.rs` (`CellData`, `PlacedRef`,
  `StaticObject`, `LightData`, `TeleportDest`, …), `walkers.rs`
  (`parse_cell_group`, `parse_refr_group`, `parse_land_record`),
  `support.rs`, `helpers.rs`, `wrld.rs`, and `tests/`.
- `crates/plugin/src/esm/strings_table.rs` — `StringsTable` / `StringTableSet`
  for the Skyrim+ localized `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` split.
- `crates/plugin/src/equip.rs` — biped-slot bit constants + equip-slot
  resolution (xEdit-derived; see the crate docstring's acknowledgement).
- `crates/plugin/src/{datastore,manifest,record,resolver}.rs` — the
  Redux-native plugin tier (`DataStore`, `PluginManifest`, `Record`,
  `DependencyResolver`).
- `crates/plugin/src/legacy/mod.rs` — the `pub(crate)` LegacyFormId /
  load-order bridge. Forward-looking scaffolding; audit for rot, not for
  correctness against a consumer that doesn't exist.

**Not in scope** (owned elsewhere, cross-reference only): the cell loader's
consumption of `CellData` (`/audit-<game>` per-game Dimension 1), Papyrus
`VMAD` translation (`/audit-scripting` Dim 7), NIF/BSA (`/audit-nif`).

**Ground truth — read before auditing**:
- `docs/engine/plugin-loading.md` — manifest schema, `DataStore`,
  `DependencyResolver`, the Form ID three-layer design.
- `docs/engine/pipeline-overview.md` — the ESM record → ECS spawn → GPU draw
  trace this crate is the head of.
- The `crates/plugin/src/lib.rs` docstring's **xEdit acknowledgement**: every
  non-obvious per-record decode is supposed to cite a
  `wbDefinitions{TES4,FNV,TES5,FO4,FO76,SF1}.pas:line` range. A decode with no
  citation and no test is a guess — see `feedback_no_guessing`; flag it.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers. Default: all 8.
- `--game <name>`: restrict real-data validation (Dim 8) to one game's masters.
  Default: every game with on-disk data (`_audit-common.md` § Game Data).
- `--depth shallow|deep`: `shallow` = schema/contract check; `deep` = byte-level
  trace against real masters. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Header & GRUP Walk | Sub-Record Byte Accounting | FormID &
  Load Order | Record Schema Dispatch | CELL / WRLD Walkers | Localized Strings
  | ESM→ECS Handoff | Real-Data Validation
- **Record / Sub-record**: the 4-char code(s) the finding concerns (e.g. `NPC_`
  / `ACBS`), or `—`.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--game`, `--depth`.
2. `mkdir -p /tmp/audit/esm`.
3. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/esm/issues.json`.
4. Read the most recent `docs/audits/AUDIT_ESM_*.md` if one exists, plus the
   ESM dimensions of the most recent per-game reports (`AUDIT_FNV_*`,
   `AUDIT_SKYRIM_*`, `AUDIT_FO4_*`, `AUDIT_STARFIELD_*`) — this crate's
   findings historically live there, so **that is where your duplicates are**.
5. `cargo test -p byroredux-plugin` and record the pass count. A pre-existing
   failure is context for every dimension, not a finding of its own.

## Phase 2: Launch Dimension Agents

### Dimension 1: Header Detection & GRUP Walk (highest blast radius)
**Entry points**: `crates/plugin/src/esm/reader.rs` — `EsmVariant::detect`,
`record_header_size`, `group_header_size`, `GameKind::from_header`,
`EsmReader::read_file_header`, `EsmReader::bounded_group_content_end`,
`MAX_GRUP_NESTING_DEPTH`; `crates/plugin/src/esm/records/grup_walker.rs`
**Why first**: a header-size or GRUP-bounds error desynchronizes the whole file.
Everything downstream then decodes garbage that *looks* structurally valid.
**Checklist**:
- `EsmVariant::detect` is the one-shot `data[20..24] == b"HEDR"` test
  (Oblivion = 20-byte headers, everything else = 24). Verify the length guard
  precedes the slice, and that no caller bypasses detection with a hardcoded
  variant outside tests / `with_variant`.
- `GameKind::from_header` maps the HEDR `Version` f32 to a game through
  **banded** comparisons, never float equality. The bands must leave clear gaps
  between the sampled vanilla values (FO3 0.94, FO4 1.0, Starfield 0.96,
  FNV 1.34, Skyrim SE 1.71, FO76 68.0). This mapping was **inverted once
  already** (#439 / FO3-3-01, FO3↔FO4) and was latent because the item DATA
  arms bucket FO4 with FO3NV — re-verify each band end-to-end, and check
  whether any *new* schema split has since made a mis-band non-latent.
- A missing/short HEDR falls back to `GameKind::Fallout3NV` (the `Default`).
  Confirm the fallback is deliberate at every call site and that no per-game
  branch silently treats "defaulted" as "detected FO3".
- GRUP walking: every group advances by `group_header_size()` and is bounded by
  the group's own declared size, clamped to the parent's end. Verify a lying
  size field cannot walk past the buffer or loop forever (zero/negative advance).
- Nested GRUPs (world children, cell children, topic children) recurse with a
  depth bound (#3237): every recursive walker's `sub_end` comes from
  `EsmReader::bounded_group_content_end(header, depth, walker_name)`, which
  returns `None` (and calls `skip_group` internally) once `depth >=
  MAX_GRUP_NESTING_DEPTH` (64) instead of handing back an end offset to
  recurse into — a malformed file with deeper nesting is skipped, not
  stack-overflowed. Confirmed wired into
  `grup_walker.rs::{extract_records, extract_records_with_modl,
  extract_dial_with_info, extract_quest_dialogue_scene_tree_inner}` and
  `cell/walkers.rs::parse_cell_group` + `cell/wrld.rs::parse_wrld_children`
  (Dimension 5) — each threading a `depth + 1` argument through its `_inner`
  recursion. This is **not yet universal**: `cell/support.rs`'s
  `parse_modl_group`, `parse_ltex_group`, `parse_txst_group`,
  `parse_scol_group`, `parse_pkin_group`, `parse_movs_group`, and
  `parse_mswp_group` all still recurse on a raw, unbounded
  `reader.group_content_end(&sub)`, as does `cell/walkers.rs::parse_refr_group`
  (see Dimension 5) — check whether that gap has been closed, and if not,
  flag it as the live instance of exactly the stack-overflow risk this bullet
  exists to catch, not as a new finding invented from scratch.
- Record flag `FLAG_COMPRESSED`: the 4-byte uncompressed-size prefix is read
  first, `data_size - 4` is the zlib payload. Verify `data_size >= 4` is checked
  *before* the subtraction (an underflow here is a panic on hostile input) and
  that the decompressed buffer is capacity-hinted, not trusted — a lying prefix
  must not pre-allocate unbounded memory.
**Output**: `/tmp/audit/esm/dim_1.md`

### Dimension 2: Sub-Record Byte Accounting (the densest bug class)
**Entry points**: `crates/plugin/src/esm/sub_reader.rs` — `SubReader` and its
`u8`/`u16`/`u32`/`i16`/`i32`/`f32`/`fixed`/`f32_array`/`rgb_color`/`rgba_color`
readers, the `*_or_default` family, `skip`, `skip_or_eof`, `rest`, `remaining`;
every `misc/*.rs` decoder that consumes them
**Why this dimension**: this is the ESM analogue of `/audit-nif` Dimension 1
(stream-position integrity), and it has the same failure mode — a wrong field
width shifts every later field in the same sub-record, and the result parses
"successfully" with wrong numbers.
**Checklist**:
- Every per-record decoder reads through `SubReader`, not by indexing the raw
  slice. Flag direct `data[n..m]` slicing in a decoder — that is where the
  panics and the silent truncations both come from.
- The `*_or_default` family swallows short reads. It exists for genuinely
  optional tails; using it in the middle of a fixed-layout struct converts a
  truncation into silent zeros. For each call site, decide: optional tail (OK)
  or mid-struct (finding).
- Fixed-size sub-records: does the decoder check the declared sub-record length
  against the schema before decoding, or does it decode-then-hope? A DATA that
  is 4 bytes shorter than expected in one game version is exactly the FO3/FNV vs
  Skyrim vs FO4 divergence `GameKind` exists to resolve.
- Version-gated tails: a field added in a later game must be gated on
  `GameKind` (or on remaining length), never read unconditionally. Cross-check
  against the xEdit citation in the decoder's comment; a gate with no citation
  and no test is a guess.
- Worked instance: `crates/plugin/src/esm/records/actor/mod.rs::parse_race`'s
  TES5 (`GameKind::Skyrim`, 128/164 B) `DATA` arm decodes three back-to-back
  `f32`s at the fixed offsets 36/40/44 into `RaceRecord.starting_health` /
  `starting_magicka` / `starting_stamina` (all `Option<f32>`, gated
  finite-and->0), immediately after the 36-byte
  skills(14)+padding(2)+height(8)+weight(8)+flags(4) head. Verify a future
  field added to this tail lands at offset 48 and beyond in sequence, not by
  reusing 36/40/44, and that it stays scoped to the TES5 arm — the sibling
  TES4/FO3NV arm (8-slot skill array, no starting-pool floats at all) and the
  FO4/FO76 arm (200/216 B, no skill-bonus array, floats from offset 0) must
  not be confused with it. The CHARAL-side consumption of these two new
  fields is `/audit-character` Dimension 5's territory, not this one's — only
  the byte offsets are in scope here.
- `rgb_color` vs `rgba_color`: 3-byte vs 4-byte reads. A swapped pair shifts
  every subsequent field by one byte. Grep both and confirm each against the
  record's documented layout — and remember `feedback_color_space`: these are
  raw monitor-space floats, do **not** flag a missing sRGB conversion.
- `crates/plugin/src/esm/records/misc/water.rs::decode_data_fo3nv` — WATR's
  full FO3/FNV `DATA` shares its first 28 bytes with the Oblivion/synthetic
  short form: `wind_speed`/`wind_direction`/`wave_amplitude`/`wave_frequency`
  at offsets 0/4/8/12 (direction is degrees-on-the-wire, converted once),
  then `sun_specular_power`/`reflectivity`/`fresnel` at 16/20/24 (see the
  docstring's offset table). This shared prefix has been misattributed twice
  before (#3107 routed it through the per-record noise-layer-1 tail at
  offset 100/112 and the tail-derived amplitude/frequency at 76/80; #3144 was
  a degrees→radians double-conversion; #3205 settled it back to the
  independent 0..28 prefix — `apply_fo3nv_tail`'s comment at the noise-layer
  block now says so explicitly). Verify against the current docstring table
  and this file's `tests` module —
  `parse_watr_186_byte_record_reads_colors_at_40_44_48` and
  `wind_direction_converts_shipped_degrees_on_the_fo3nv_dnam_arm` both assert
  the current 0..28-prefix semantics — not against an older audit report's
  description of the offset map: this field has genuinely moved twice.
- Null-terminated vs length-prefixed strings (`EDID`, `FULL`, `MODL`): verify
  the terminator is consumed exactly once and a missing terminator can't read
  past the sub-record.
- Repeating-row sub-records (`CTDA` lists, `CNTO` inventories, `NVTR`/`NVEX`
  navmesh rows, `decode_nvtr_row` / `decode_nvex_row`): row count must be
  derived from `len / stride` with an explicit remainder check. A non-zero
  remainder means the stride is wrong for this game — flag it as a decode bug,
  not a tolerated leftover.
**Output**: `/tmp/audit/esm/dim_2.md`

### Dimension 3: FormID Remap, Load Order & ESL Space
**Entry points**: `crates/plugin/src/esm/reader.rs` — `FormIdRemap::remap`,
`GlobalSlot::compose`, `EsmReader::set_form_id_remap`;
`crates/plugin/src/esm/records/mod.rs` — `parse_esm_with_load_order`;
`byroredux/src/cell_loader/load_order.rs`
**Checklist**:
- `GlobalSlot::Regular` keeps the low 24 bits and writes the byte index;
  `GlobalSlot::Light` packs a 12-bit sub-index into the `0xFE` space and keeps
  only the low **12** bits. Verify the ESL masks (`0x0FFF` both sides) — using
  the 24-bit mask for an ESL collapses distinct forms onto one global id.
- `remap` has four arms: self-reference (`mod_index == master_slots.len()`),
  in-range master, standalone-with-no-masters (pass through, `debug` log — the
  known vanilla Oblivion `0x01` authoring artifact, #1308/OBL-D6-NEW-04), and
  out-of-range-with-masters (pass through, `warn`). Verify all four survive, and
  in particular that the standalone arm has **not** been "fixed" into a clamp —
  the comment explains why clamping is strictly worse (two forms colliding on
  one global id inside `EsmIndex`).
- Every `HashMap<u32, _>` in `EsmIndex` is keyed by the **remapped** id. Find any
  decoder that stores a raw plugin-local id into the index, or that compares a
  remapped key against a raw reference — that's a cross-plugin dangling ref.
- Master-list order (`MAST` sub-records of TES4) defines `master_slots`. Verify
  the list is read in file order and that a missing master produces a diagnosable
  failure rather than a silently shifted slot table.
- Multi-plugin: `parse_esm` passes `None`. Confirm which CLI paths actually wire
  a `FormIdRemap` today (`--master` is repeatable) and whether the docstring's
  "current CLI only wires a single plugin" claim still holds — if the CLI grew
  multi-plugin support and the docstring didn't, that's doc rot (report it).
**Output**: `/tmp/audit/esm/dim_3.md`

### Dimension 4: Record Schema Dispatch & Coverage
**Entry points**: `crates/plugin/src/esm/records/mod.rs` (the GRUP label →
`dispatch_*` routing), the eight `crates/plugin/src/esm/records/dispatch_*.rs`
routers, `crates/plugin/src/esm/records/index.rs` (`EsmIndex`)
**Checklist**:
- Build the live matrix: for each 4-char record type routed by a `dispatch_*`
  arm, does it (a) decode into a typed record, (b) decode into a stub, or (c)
  get skipped? Compare against the record catalog in the project memory
  (*record_type_catalog*) and report the delta as coverage, not as bugs.
- `dispatch_misc_stub.rs` is the "recognized but not decoded" bucket. Verify a
  stub still advances the reader correctly — a stub that mis-advances is worse
  than no arm at all.
- The unknown-record path must skip by declared size and continue. Verify it
  never logs per-record at `warn` on a vanilla master (log spam masks real
  diagnostics on a 200k-record file).
- Per-game schema splits inside one record type (ARMO/WEAP/AMMO `DATA`/`DNAM`,
  `BOD2` vs `BMDT`, FO4 `SCOL`/`PKIN`/`TXST`): each split must switch on
  `GameKind`, and the FO4-bucketed-with-FO3NV shortcut noted in the
  `GameKind::from_header` comment must still be true for every arm that relies
  on it. If a new split landed that breaks that assumption, the stale
  `from_header` band becomes live — HIGH.
- Duplicate FormIDs across a multi-record group: last-write-wins into the
  `HashMap`. Confirm that matches the Bethesda override semantics the
  `DataStore`/`DependencyResolver` tier documents, and that `merge_from`
  (multi-plugin) applies the same rule.
- `crates/plugin/src/esm/records/actor_value_derive.rs` is the CHARAL feed —
  cross-reference `/audit-character` Dim 4 rather than re-auditing the formulas.
**Output**: `/tmp/audit/esm/dim_4.md`

### Dimension 5: CELL / WRLD Walkers & Placement Data
**Entry points**: `crates/plugin/src/esm/cell/walkers.rs` —
`parse_cell_group`, `parse_refr_group`, `parse_land_record`;
`crates/plugin/src/esm/cell/mod.rs` — `CellData`, `PlacedRef`, `StaticObject`,
`LightData`, `TeleportDest`, `PrimitiveBounds`, `EnableParent`, `PortalLink`,
`LinkedRef`, `TextureSlotSwap`, `LandscapeData`, `TerrainQuadrant`;
`crates/plugin/src/esm/cell/wrld.rs`
**Checklist**:
- The four CELL child sub-groups (persistent / temporary / distant / VWD, per
  *cell_record_structure* in memory) must each be walked, and a REFR's group
  membership must survive into `PlacedRef` — the persistent/temporary split is
  what streaming and save-restore both key on.
- GRUP nesting depth bound (#3237, cross-referenced from Dimension 1):
  `parse_cell_group` (`walkers.rs`) and `parse_wrld_children` (`wrld.rs`) both
  route their sub-group `sub_end` through
  `EsmReader::bounded_group_content_end(..., depth, walker_name)` and thread
  `depth + 1` into their `_inner` recursion, so a file nesting CELL/WRLD
  groups past `MAX_GRUP_NESTING_DEPTH` (64) is skipped rather than
  stack-overflowed. `parse_refr_group` (same file, also an entry point above)
  was **not** updated alongside them — it still recurses on
  `reader.group_content_end(&sub)` with no depth counter. Verify whether that
  gap is still open; if so it is the live regression case for this bullet,
  not a hypothetical.
- Lighting-template inheritance is **per-field**, not all-or-nothing. Verify the
  inherit flags are applied field-by-field and that "absent" and "authored zero"
  stay distinguishable.
- `XCLW` water height is a **tri-state**: absent → inherit the WRLD default, a
  finite value → override, and either no-water sentinel (`INT_MIN` or Skyrim's
  `FLT_MAX`) → explicitly suppress the plane (see `docs/engine/watal.md`).
  Verify all three survive the CELL boundary; collapsing the sentinel to
  "inherit" puts a water plane in a dry cell.
- Exterior grid: cell `(x, y)` from `XCLC`, worldspace parenting and the
  selective-inheritance flags in `wrld.rs`. Verify a child worldspace inherits
  only the flagged categories.
- `parse_land_record`: quadrant/layer counts and the splat-alpha rows are
  fixed-stride; a stride error here is invisible in code and obvious on screen.
  Cross-reference `/audit-<game>` terrain dimensions rather than duplicating.
- Navmesh (`NAVM`): the classic per-sub-record path (`NVTR`/`NVEX`) and the
  Creation-Engine packed `NVNM` body (#2738, `decode_nvnm`) must produce the
  same canonical `NavmRecord`. Verify the shared row decoders
  (`decode_nvtr_row`, `decode_nvex_row`) are genuinely shared, that the cursor
  refuses to read past the blob end, and that `NVNM_MAX_DIVISOR` bounds the
  grid-divisor read. Retained-raw-bytes-on-failure must be a *diagnosable*
  fallback, not a silent success.
**Output**: `/tmp/audit/esm/dim_5.md`

### Dimension 6: Localized Strings (Skyrim+ `.STRINGS` family)
**Entry points**: `crates/plugin/src/esm/strings_table.rs` — `StringsTable::parse`,
`StringsTable::get`, `StringTableSet::load`, `StringTableSet::resolve`;
`StringsTableGuard`
**Checklist**:
- The TES4 header's localized flag decides whether a `FULL`/`DESC` payload is a
  literal string or a `u32` string id. Verify the branch is driven by the flag,
  not by "looks like a small integer".
- `.STRINGS` has no length prefix; `.DLSTRINGS`/`.ILSTRINGS` do
  (`has_length_prefix`). Verify each file kind is parsed with the right form and
  that a mismatch fails loudly instead of yielding shifted garbage.
- Missing string files (a load order without the language pack) must degrade to
  "no string" — verify no panic and no per-form warn spam.
- Language selection defaults and the `resolve` miss path: an unresolved id
  should be diagnosable in one place, not become an empty `String` at 40 call
  sites.
**Output**: `/tmp/audit/esm/dim_6.md`

### Dimension 7: `EsmIndex` → ECS Handoff & the Redux-Native Tier
**Entry points**: `crates/plugin/src/esm/records/index.rs` (`EsmIndex`,
`merge_from`), `crates/plugin/src/record.rs`, `crates/plugin/src/datastore.rs`,
`crates/plugin/src/resolver.rs`, `crates/plugin/src/manifest.rs`,
`crates/plugin/src/equip.rs`; consumers `byroredux/src/cell_loader/references/`
and `byroredux/src/npc_spawn.rs`
**Checklist**:
- `EsmIndex` is a bag of `HashMap<u32, _>` held for the session. Check its
  growth on a large load order (every NPC, item, script record retained) against
  `docs/engine/memory-budget.md` — this is RAM, not VRAM, and nothing evicts it.
- Every `Option<u32>` FormID field on a record is a reference that must resolve
  through the same remapped space. Sample the high-traffic ones (`SCRI`,
  `RNAM`, `CNAM`, `PKID`, `XEZN`, teleport `XTEL`) and confirm the resolver used
  by the cell loader is `resolve_entity_by_global_form_id` (per
  *m47_scripting_state*) — **not** a raw `World::find_by_form_id`.
- `equip.rs` biped-slot constants: per-game bit meanings differ (BMDT vs BOD2 vs
  FO4). Verify each constant block cites its xEdit definition and that the
  slot→`addon_index` mapping (`AddonData::addon_index`,
  `crates/plugin/src/esm/cell/mod.rs`) matches *equipment_system* in memory.
- The Redux-native tier (`manifest`/`record`/`datastore`/`resolver`) is
  forward-looking: `DependencyResolver`'s DAG and `Conflict` reporting have few
  or no live callers. Audit it for **rot** (does it still compile against the
  current `Record` shape, are its docs consistent with
  `docs/engine/plugin-loading.md`) and flag dead-but-documented API — do not
  report "unused" as a bug on its own.
- `legacy/mod.rs` is `pub(crate)` scaffolding with a documented rationale
  (#1322). Same treatment: rot only.
**Output**: `/tmp/audit/esm/dim_7.md`

### Dimension 8: Real-Data Validation
**Entry points**: the on-disk masters listed in `_audit-common.md` § Game Data
Locations; `crates/plugin/examples/` probes; `byroredux/src/list_cells.rs`;
`byroredux/src/sf_smoke.rs` (Starfield resolve-rate harness)
**Checklist**:
- For each `--game`, parse the vanilla master and record: records seen, records
  decoded, records stubbed, records skipped, and *unknown sub-record codes per
  record type*. The last number is the coverage signal that per-game audits
  never compute across the whole file.
- Any record type whose decode rate drops between games is a schema-split
  suspect — correlate with `GameKind` arms from Dim 4.
- Compare Starfield against the `--sf-smoke` resolve-rate baseline; a drop is a
  regression, not a new finding.
- Report parse **time** and peak RSS per master. `/audit-performance` Dim 8
  owns NIF parse cost; ESM parse cost has no owner and is on the critical path
  of every cell load.
- Do not launch a windowed engine instance for this dimension
  (`feedback_no_parallel_engine_launch`) — use the example/probe binaries and
  `cargo test -p byroredux-plugin`, or read only.
**Output**: `/tmp/audit/esm/dim_8.md`

## Phase 3: Merge

1. Read all `/tmp/audit/esm/dim_*.md`.
2. Combine into `docs/audits/AUDIT_ESM_<TODAY>.md`:
   - **Executive Summary** — findings by severity; the crate's coverage posture
     (record types decoded / stubbed / skipped per game); explicit statement of
     which games' real data was actually parsed.
   - **Record Coverage Matrix** — record type × game × {decoded, stubbed,
     skipped, unknown-subrecords}. This table is the durable artifact; keep it
     even when there are no findings.
   - **Findings** — grouped by severity, deduplicated.
   - **Cross-Audit Pointers** — what belongs to `/audit-<game>` Dim 1, to
     `/audit-scripting` (VMAD), to `/audit-character` (AVIF/class), and to
     `/audit-physics` (collision-relevant placement data).
3. Deduplicate against the per-game reports read in Phase 1 — a finding already
   filed under an `/audit-<game>` ESM dimension is `Existing: #NNN`, not new.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/esm`
2. Inform the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_ESM_<TODAY>.md`
   (domain label: `esm-plugin`; `import-pipeline` only for BSA/BA2 archive findings,
   plus the matching `game:*` when the finding is specific to one title's records).
