# SpeedTree Subsystem Audit — 2026-06-14

Audit of the `byroredux-spt` crate (Session 33 Phase 1 "S1"): the `.spt`
parameter-section TLV walker + placeholder-billboard import fallback, plus
the cell-loader / loose-file wiring that invokes it.

- **Skill**: `/audit-speedtree`
- **Depth**: deep (source contract review + on-disk corpus harness)
- **Focus**: all dimensions (1–6)
- **Scope**: `crates/spt/src/{parser,tag,version,stream,scene}.rs`,
  `crates/spt/src/import/mod.rs`; cross-cuts
  `byroredux/src/cell_loader/references.rs`,
  `byroredux/src/cell_loader/spawn.rs`,
  `byroredux/src/scene/nif_loader.rs`,
  `crates/plugin/src/esm/records/tree.rs`,
  `byroredux/src/systems/billboard.rs`,
  `crates/core/src/ecs/components/billboard.rs`.

## Result

**No new findings.** Every candidate issue examined was either disproved on
close reading of the current code path, or is a documented-intentional
behaviour (future-phase ride-through, defensive edge case) that the skill
explicitly directs not to flag.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |
| **TOTAL**| **0** |

## Verification performed

### Dedup
- `gh issue list … --search "speedtree OR spt OR TREE"` → `[]` (no open
  SpeedTree-tagged issues). Full 200-issue dump scanned for
  `spt`/`speedtree`/`tree`/`billboard`/`tlv`/`tag`/`idvspt` keywords across 23
  open issues — none match this subsystem.
- Prior report `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md` reviewed. Its
  findings SPT-D1-01 / D2-01 / D3-01 / D4-01/02/03/04 / D5-01/02 are all
  closed (#994–#1002). Each was checked as a regression guard (below) — none
  have broken.

### Tests
- `cargo test -p byroredux-spt` → 45 lib + 3 synthetic pass, 0 fail.
- Clean `cargo build -p byroredux-spt` — no warnings, no dead code.

### Deep corpus harness (acceptance gate, Dimension 1)
`cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored`
against on-disk vanilla BSAs:

| Game | Files | Clean | Unknown-tag bail | Coverage |
|------|-------|-------|------------------|----------|
| FNV  | 10    | 10    | 0                | 100.00 % |
| FO3  | 10    | 10    | 0                | 100.00 % |
| OBL  | 113   | 113   | 4                | 96.46 %  |

All three clear the ≥ 95 % gate. The 4 Oblivion bails are the known
tag-13005 BezierSpline outliers (`treems14canvasfreesu`, `treecottonwoodsu`,
`shrubms14boxwood`, `treems14willowoakyoungsu`) — handled by the
`MaybeStringElseBare` heuristic; the file still produces a placeholder.
20,425 entries decoded across the corpus with zero hard parse errors.

## Dimension-by-dimension bill of health

### Dimension 1 — Walker Byte-Accounting (highest risk): CLEAN
- Every `SptTagKind` decode in `read_payload` (`parser.rs:140-192`) advances
  the cursor by exactly the bytes claimed: `U8`=1, `U32`=4, `Vec3`=12,
  `FixedBytes(n)`=n, `String`=4+len, `ArrayBytes{stride}`=4+count×stride,
  `Bare`=0. Cross-checked against the `tag.rs` dictionary — no size mismatch.
- `MaybeStringElseBare` (13005, #999) re-syncs cleanly on both arms: the tag
  u32 is consumed, the next u32 peeked; Bare arm leaves the peeked tag for the
  next loop, String arm's `read_string_lp` reads that same u32 as the length
  prefix (no double-consume). `None` peek (EOF) routes to String → length-read
  fails with `UnexpectedEof` (no panic) — `tag_13005_at_eof_does_not_panic`
  guard holds.
- Walker stops cleanly at `is_eof()` (sets `reached_eof`) and at
  out-of-range/`Unknown` tags (records `tail_offset` + `unknown_tags`), never
  reading one tag past EOF.
- 64 KiB sanity caps confirmed on the **byte count** (count × stride for
  `ArrayBytes`, length for `read_string_lp`), not just `count`
  (`parser.rs:160-168`, `stream.rs:97-107`).
- `parse_spt` returns `Err` only on the two fatal conditions (missing magic,
  mid-payload underflow); in-range-unknown tags are non-fatal. No new fatal
  path introduced. LE-only readers throughout `stream.rs`.

### Dimension 2 — Placeholder Fallback Correctness: CLEAN
- `import_spt_scene` (`import/mod.rs:116-186`) has no `Err` path — always a
  one-node/one-mesh `ImportedScene`. The only `None` to the cell loader is
  `parse_spt` `Err`, which `parse_and_import_spt` logs + returns `None` for,
  skipping that REFR without aborting the cell (graceful-degradation contract
  intact, `references.rs:1173-1176`).
- Leaf-texture precedence ICON → tag-4003 → unset confirmed
  (`import/mod.rs:127-135`); both #997 guards present.
- Billboard sizing precedence **OBND → BNAM → MODB → 256×512**, all clamped
  `[16, 8192]` (`compute_billboard_size`, `import/mod.rs:209-226`). OBND-beats-
  BNAM and MODB-Oblivion-path guards (#1001/#1002) all present and asserting.
- Normal `-Z` + winding `[0,3,2,2,1,0]` (#1000) match the billboard arc
  `from_rotation_arc(-Z, look_dir)` in `billboard.rs:142-146`; both geometric-
  normal guards present.
- `bs_bound` Z-up→Y-up via `zup_to_yup_pos` + half-extent reshuffle `(hx,hz,hy)`
  (#995, `import/mod.rs:168-178`); width(X)/height(Z) sizing is consistent with
  the bound reshuffle.
- Two-sided alpha-test cutout flags exact (`alpha_test:true`,
  `alpha_threshold:0.5`, `alpha_test_func:6`, `has_alpha:false`).
- `BsRotateAboutUp` world-up yaw-lock approximation in `billboard.rs:124-136`
  is unchanged and documented (no silent drift into pitch).

### Dimension 3 — TREE → Billboard Wiring: CLEAN
- `.spt` route fires on a case-insensitive `.spt` extension on the REFR base's
  MODL (`references.rs:512-517`); TREE record fetched from
  `record_index.trees` by `child_form_id`. Mixed `.nif`/`.spt` REFRs coexist
  (the branch is per-model inside the shared parse path).
- `parse_and_import_spt` returns the same `CachedNifImport` shape with the
  synthetic defaults the generic spawn must not mis-read:
  `placement_root_billboard = Some(BsRotateAboutUp)` (#994), `bsx_flags = 0`
  (#1214), `root_flags = 0` (#1235), `flame_attach_offset = None`
  (`references.rs:1239-1259`). `spawn.rs:230-231` inserts `Billboard` only when
  `placement_root_billboard.is_some()`; `bsx_flags`/`root_flags` guarded by
  `!= 0` so the placeholder skips both.
- `TreeRecord` capture lossless for OBND→`bounds`, ICON→`leaf_texture`,
  MODB→`bound_radius`, BNAM→`billboard_size`. CNAM is shape-tolerant across the
  Oblivion-5-float vs FO3/FNV-8-float split (`tree.rs:165-174`, reads every
  full f32) — not mis-parsed. SNAM/CNAM parsed-but-not-consumed (TD5-011), not
  a drop.
- `.spt` resolves through the same `extract_mesh` + `normalize_mesh_path` chain
  as `.nif` (extension-agnostic, `asset_provider.rs:92-136`) — no parallel
  resolver.
- Placeholder mesh becomes an ordinary `MeshHandle` entity parented to the
  placement root; cell unload drops its BLAS via the standard refcount path
  (`unload.rs`), no special-cased leak.

### Dimension 4 — Per-Game Variants & Route Divergence: CLEAN
- `detect_variant` recognises any `__IdvSpt_02_`-prefixed file, defaults to
  `V5Fnv`, rejects everything else as `Unknown` (`version.rs:90-100`). Nothing
  downstream branches on the variant — the placeholder path is variant-agnostic
  (`detect_variant` is not even called on the production cell route; the route
  uses `parse_spt` which gates on `MAGIC_HEAD` directly). Both guards present.
- One-byte magic flip + short-prefix rejection confirmed
  (`detect_variant_recognises_idvspt_magic`).
- Route divergence understood and documented: cell route threads TREE metadata
  (`references.rs:1183-1223`); loose `--tree` route uses
  `SptImportParams::default()` (`nif_loader.rs:194-198`). **Both call
  `parse_spt` + `import_spt_scene`** — no parse-call drift. The loose route
  still attaches `Billboard` from the placeholder root's `billboard_mode`
  (`nif_loader.rs:437-439`), so it is functionally equivalent for rendering;
  the only difference is sizing/texture metadata, which is by design.

### Dimension 5 — Tag Dictionary: CLEAN
- `dispatch_tag` spot-checked against `format-notes.md` (2026-05-09 table) and
  the `tag.rs` unit tests: 8003/8005/8009 = 52 B, 13008 = 11 B, 13013 = 7 B,
  12002 = 16 B, 12003 = 20 B; ArrayBytes 10002 stride 1 / 10003 stride 8. All
  match the observed histogram. Confounder tags (4096, 5376, …) stay `Unknown`
  (`unknown_for_out_of_dictionary_tags`). No size contradicts the corpus.
- Conservative-by-design (~90 mapped tags); the stale "~14 tag" framing does
  not appear — dictionary size correctly not treated as a gap.

### Dimension 6 — NIFAL Material Translation for Placeholders: CLEAN
- Placeholder `ImportedMesh` is canonicalised at the single `translate_material`
  boundary via `spawn.rs` for both routes — no parallel "spt material" path.
- Non-PBR defaults all present and correct (`import/mod.rs:319-361`):
  `is_pbr:false`, `from_bgsm:false`, `metalness_override:None`,
  `roughness_override:None` (#1076/#1077), `bgsm_greyscale_lut_path:None`
  (#1353), `emissive_source:None` (#1280). Cross-cut struct additions kept in
  lockstep: the most recent edit (411ca9b0, today) correctly added
  `ragdoll:None` to the placeholder `ImportedScene` literal alongside
  `attach_points`/`child_attach_connections`/`embedded_clip` = `None`.

## Candidate issues examined and disproved

- **13005-at-true-EOF mis-classified as String → `Err`.** Defensive edge
  documented in `tag.rs` and guarded by `tag_13005_at_eof_does_not_panic`; the
  corpus shows 13005 is never the final pre-EOF tag in vanilla content (it is
  always followed by more parameter data or the geometry tail). Non-fatal,
  intentional. Not a finding.
- **`SptImportParams.wind` / `form_id` never read by the importer.**
  Forward-looking Phase 2 ride-through fields (wind animation, per-tree
  variation seed) — the skill explicitly directs not to flag future-phase
  plumbing. Not a finding.
- **`BsRotateAboutUp` falls back to world-up yaw-lock (lacks local frame).**
  Unchanged from #994's accepted approximation; documented in `billboard.rs`,
  visually correct for tree imposters. Not a finding.
- **`extract_mesh` does not call `strip_build_prefix` for `.spt`.** `.spt`
  exists only in pre-Skyrim BSAs which carry no AE pipeline prefix; the strip is
  irrelevant to this format. Not a finding.

## Regression guards confirmed intact

#994 (billboard component attach), #995 (Z-up→Y-up bs_bound), #997 (leaf-tex
first-wins), #999 (13005 bimodal), #1000 (-Z normal + winding), #1001 (MODB
Oblivion sizing), #1002 (OBND>BNAM precedence + clamps), #1076/#1077 (non-PBR),
#1214 (bsx_flags=0), #1235 (root_flags=0), #1241/#1280/#1353 (no
BGSM/emissive/LUT) — all present and asserting.

## Suggested next step

No issues to publish. The subsystem is healthy; re-run this audit if the
geometry-tail decoder (Phase 2) lands or if `ImportedScene`/`ImportedMesh`
gain new fields the placeholder must mirror.
