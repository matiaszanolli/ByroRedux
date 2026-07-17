# SpeedTree Subsystem Audit — 2026-07-16

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker + placeholder-billboard import fallback (Session 33 Phase 1, "S1"),
plus its cross-cut wiring in `byroredux/src/cell_loader/references/mod.rs`,
`byroredux/src/cell_loader/references/import.rs`,
`byroredux/src/cell_loader/spawn.rs`, `byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`, `crates/plugin/src/esm/records/mod.rs`,
`crates/plugin/src/esm/records/grup_walker.rs`, `crates/plugin/src/esm/cell/support.rs`,
and `byroredux/src/systems/billboard.rs`.

**Depth**: `deep` — corpus acceptance harness run live against on-disk
FNV / FO3 / Oblivion BSAs; full crate unit + integration suite run;
every carried-forward finding re-verified against current source and
`gh issue list`; `git log --since=2026-07-03` walked across every
in-scope file (13-day window since the last audit) to confirm nothing
relevant regressed underneath the prior clean bill of health.

**Method**: Read the full crate (`parser.rs`, `stream.rs`, `tag.rs`,
`scene.rs`, `version.rs`, `import/mod.rs`), the cross-cut wiring files
(noting `references.rs` was split into a `references/` submodule
directory by #1877 since the last audit — `parse_and_import_spt` now
lives in `references/import.rs`, not `references/mod.rs`), ran the unit
suite and the `--ignored` corpus gate, cross-checked every prior
finding's issue number, and traced the new VWD-marker wiring (#1889)
through the TREE record path specifically since it landed after the
last audit and TREE records are dual-target (`index.statics` +
`index.trees`).

---

## Verification runs (this audit)

### Corpus acceptance gate — live run

```
BYROREDUX_FNV_DATA=… BYROREDUX_FO3_DATA=… BYROREDUX_OBL_DATA=… \
  cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture
```

```
[FO3] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[FNV] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[OBL] 113 files | 113 with entries| 4 hit unknown tag | 20425 entries  | 96.46 % coverage
  unknown-tag samples:
    trees\shrubms14boxwood.spt         | tag=768 (0x0300) at offset 4507
    trees\treecottonwoodsu.spt         | tag=768 (0x0300) at offset 5641
    trees\treems14willowoakyoungsu.spt | tag=768 (0x0300) at offset 5946
    trees\treems14canvasfreesu.spt     | tag=768 (0x0300) at offset 6211
```

Byte-for-byte identical to the 2026-07-03 run (same files, same offsets,
same coverage rates). All three gates clear the ≥ 95 % floor.

### Unit + integration suite

`cargo test -p byroredux-spt --release` — 46 unit tests + 3 synthetic
integration tests, all pass, 0 failures. `parse_synthetic_spt.rs`'s
byte-pinned regression fixture still round-trips.

---

## Dedup pass (mandatory)

`gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json
number,title,state,labels --search "speedtree OR spt OR TREE"` returns
three hits, only one SpeedTree-specific:

| Issue | State | Title |
|---|---|---|
| #1822 | OPEN | SPT-NEW-07: `MaybeStringElseBare` (tag 13005) can misparse a bare 13005 sitting immediately before the geometry tail as a length-prefixed string |
| #1857 | OPEN | TD1-001: `context/draw.rs` is 4265 LOC (renderer, unrelated) |
| #1576 | OPEN | SF-D4-03: Starfield BFCB component-block gap (unrelated) |

Two prior LOW findings closed since the last audit:

| Finding | Issue | State | This audit |
|---|---|---|---|
| SPT-NEW-01 (`detect_variant` dead code) | #1820 | **CLOSED** 2026-07-04 | Fix verified in place (below) |
| SPT-NEW-06 (format-notes.md byte-offset doc nit) | #1821 | **CLOSED** 2026-07-04 | Fix verified in place (below) |
| SPT-NEW-07 (13005-before-tail String misparse edge) | #1822 | OPEN | Re-verified, unchanged — skip |

### #1820 — fix verification

Commit `37b3bdef` ("Fix #1820: wire detect_variant into both
parse_spt call sites as a logged sanity check", 2026-07-04) wires
`byroredux_spt::detect_variant` into `parse_and_import_spt`
(`byroredux/src/cell_loader/references/import.rs:305-312`) and
`scene/nif_loader.rs:188`. Both call sites now log the resolved
variant tag alongside the existing parse-summary debug line; a
regression test pins the variant a vanilla `__IdvSpt_02_` fixture
resolves to. `detect_variant` is no longer dead code — it has two
production callers, both logging-only (documented as a corpus trail
for the future geometry-tail decoder, not a dispatch input). Confirmed
fixed.

### #1821 — fix verification

Commit `506e5a8a` ("Fix #1821: correct format-notes.md's tag-768
worked example to match the walker's real cursor", 2026-07-04)
corrects the doc's byte table: tag 13013 (`FixedBytes(7)`) consumes
4+7=11 bytes, landing the cursor exactly on the u32 that reads 768 at
all 4 Oblivion outlier offsets (4507/5641/5946/6211) — matching
`SptScene::unknown_tags` precisely. The doc's prior recommendation to
raise `TAG_MAX` (which would not have touched this bail — 768 is
already inside `TAG_MIN..=TAG_MAX`) was corrected. Confirmed fixed;
`format-notes.md:618` now reads accurately.

No new GitHub issues reference `spt`/SpeedTree/TREE content since the
last audit beyond these three.

---

## Fresh dimension sweep

Walked all six dimensions directly against current source, plus traced
the one cross-cut change since the last audit that touches TREE
records specifically:

- **Dimension 1 (Walker Byte-Accounting)**: `parser.rs`/`stream.rs`/
  `tag.rs` unchanged since 2026-06-09 except the #1819 fix (already
  verified in the prior audit). Cursor accounting, `MaybeStringElseBare`,
  EOF/out-of-range bail, and the 64 KiB caps all read exactly as
  documented. No new fatal-error path.
- **Dimension 2 (Placeholder Fallback)**: `import_spt_scene` still
  infallible; size precedence (OBND→BNAM→MODB→default, `[16,8192]`
  clamp), `-Z` normal/winding, Z-up→Y-up `bs_bound` swap unchanged.
  Two mechanical struct-literal additions landed since the last audit
  (`004b51c7` adds `furniture_markers: Vec::new()`, `5ffb7638` adds
  `bgsm_greyscale_lut_is_alpha: false`) — both are mandatory
  compiler-forced updates to `ImportedMesh`/`ImportedNode` fields added
  elsewhere in the engine, both correctly set to inert/no-op defaults
  for the placeholder (no furniture markers, no BGSM alpha LUT on a
  SpeedTree billboard). Confirms the placeholder struct stays honest as
  the shared `Imported*` types grow new fields — not a bug, but the
  right thing to check given they touch `import/mod.rs`.
- **Dimension 3 (TREE → Billboard Wiring)**: `parse_and_import_spt`
  relocated to `references/import.rs:290-` by the #1877 file split
  (was `references.rs`/`references/mod.rs`) — logic byte-identical,
  confirmed by reading the relocated function directly. Synthetic
  defaults (`bsx_flags=0`, `root_flags=0`, `flame_attach_offset=None`,
  `attach_points=None`) all still explicit and commented with their
  originating issue numbers. `spawn.rs` still inserts `Billboard` from
  `placement_root_billboard`.
  - **New this window**: #1889/#1890/#1891 (2026-07-05, "materialise
    the VWD flag as a per-placement VisibleWhenDistant marker") added
    `stamp_visible_when_distant(world, placement_root, stat.visible_when_distant)`
    unconditionally after `spawn_placed_instances` in the shared REFR
    loop (`references/mod.rs:768`) — this fires for `.spt` placements
    too, since `stat` comes from `index.statics.get(&child_form_id)`
    and TREE records are dual-target. Traced the TREE-specific plumbing:
    `crates/plugin/src/esm/records/mod.rs:351-359` routes `b"TREE"`
    through `extract_records_with_modl`, which
    (`crates/plugin/src/esm/records/grup_walker.rs:35-42`) builds the
    `StaticObject` — including `visible_when_distant:
    header.is_visible_when_distant()` — from the *same* record header
    and sub-records used to populate `index.trees` in the same walk.
    So a TREE record's VWD flag reaches the placement root exactly the
    same way a STAT record's does; no `.spt`-specific gap. Verified
    correct, not flagged.
- **Dimension 4 (Per-Game Variants)**: `detect_variant` now has two
  production callers (see #1820 above, closed) — previously flagged as
  dead code, now resolved. `MAGIC_HEAD` exact-match still enforced by
  `bytes.starts_with(...)` in `parse_spt`, independent of
  `detect_variant`.
- **Dimension 5 (Tag Dictionary)**: Unit tests
  (`fixed_byte_payload_tags`, `string_payload_tags`, `u32_payload_tags`,
  `vec3_payload_tags`, `unknown_for_out_of_dictionary_tags`,
  `tag_13005_is_maybe_string_else_bare`) all still pass; no dictionary
  edits since the last audit. `format-notes.md`'s tag-768 worked
  example is now byte-accurate (#1821, above).
- **Dimension 6 (NIFAL Material Translation)**: `is_pbr:false`,
  `from_bgsm:false`, `emissive_source:None`, `metalness_override:
  Some(0.0)`/`roughness_override: Some(0.85)` (#1819 fix, previously
  verified), two-sided alpha-test cutout, and the single
  `translate_material` boundary all still hold on both the cell-loader
  route and the `--tree` loose route. New `bgsm_greyscale_lut_is_alpha:
  false` field (above) keeps the placeholder correctly outside the
  BGEM-LUT-alpha path it was never eligible for.

`git log --since=2026-07-03` across every in-scope file confirms 13
commits touched files in this audit's scope; of those, only two touch
`crates/spt/` itself (`004b51c7`, `5ffb7638`, both mechanical
struct-literal keep-in-sync edits, above), one is the #1877 file split
(`9f12b2eb`, mechanical relocation, logic unchanged), and one is the
#1889/#1890/#1891 VWD-marker addition (`a8d65d6c`/`d4b981fa`, traced
above — correct for TREE, not spt-specific). The remaining commits
(`c5dcad97`, `2f1a637b`, `004b51c7`'s idle-variety half, `dae33650`
NIF-cache clip-handle ordering — orthogonal, `.spt` scenes never
populate `embedded_clip`/`pending_clip_handles`, `5ffb7638`'s BGEM
half, `f5992b6b` CONT inventory, `9107dfa1` spawn-point selection,
`7d4173a1` awake-faller diagnostic, `2c58efb7` trigger-box rotation
verify) don't touch the `.spt` route at all.

No new findings surfaced.

---

## Findings

None. Both LOW findings carried from the prior audit lineage
(SPT-NEW-01 / #1820, SPT-NEW-06 / #1821) were fixed and verified in
place this window. The one remaining tracked item (SPT-NEW-07 / #1822)
is unchanged, already filed, and correctly out of scope for
re-reporting per the dedup protocol.

---

## Regression Guards (verified in place, NOT re-reported)

All prior findings remain fixed and their guards hold this audit:

| Finding | Issue | Guard verified |
|---|---|---|
| SPT-D4-01 (cell placeholder loses `Billboard`) | #994 | `spawn.rs` inserts `Billboard` when `placement_root_billboard.is_some()`; `parse_and_import_spt` sets `Some(BsRotateAboutUp)` |
| SPT-D4-02 (`bs_bound` Z-up→Y-up) | #995 | `import/mod.rs` routes center via `zup_to_yup_pos`, half-extents `(hx,hz,hy)`; `placeholder_uses_obnd_bounds_when_present` passes |
| SPT-D5-01 (`wind` docstring) | #996 | `SptImportParams.wind` doc says CNAM, not BNAM |
| SPT-D2-01 ("first wins" leaf tex) | #997 | `import/mod.rs` `.first()`; `leaf_texture_override_wins_over_spt_tag` passes |
| SPT-D3-01 (pinned regression sample) | #998 | `tests/parse_synthetic_spt.rs` byte-pinned fixture passes |
| SPT-D1-01 (13005 bimodal) | #999 | `MaybeStringElseBare`; both `tag_13005_*` guards pass (residual tail edge tracked as #1822/SPT-NEW-07) |
| SPT-D4-03 (normal/winding) | #1000 | `-Z` normals + `[0,3,2,2,1,0]` winding; both geometric-normal guards pass |
| SPT-D4-04 (default size / MODB) | #1001 | `compute_billboard_size` OBND→BNAM→MODB→default; `modb_drives_placeholder_size_when_obnd_absent` passes |
| SPT-D5-02 (BNAM precedence) | #1002 | OBND-beats-BNAM; `obnd_precedence_over_bnam` passes |
| BSXFlags dropped at spawn | #1214 | `bsx_flags = 0` synthetic default |
| SceneFlags / root_flags | #1235 | `root_flags = 0` synthetic default |
| SPT-NEW-02/03/04 doc/route | #1707/#1711/#1715 | All CLOSED; guards still in place |
| SPT-NEW-05 (foliage keyword collision) | #1819 | `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` at `import/mod.rs`; regression test passes |
| SPT-NEW-01 (`detect_variant` dead code) | #1820 | **Newly CLOSED this window** — two production callers now log the resolved variant; `parse_and_import_spt`/`nif_loader.rs` both wired |
| SPT-NEW-06 (format-notes.md byte-offset nit) | #1821 | **Newly CLOSED this window** — `format-notes.md` tag-768 worked example now byte-accurate |

The **14000-band Oblivion tail** (4 files bail at tag `768`) remains the
documented `format-notes.md` Phase-1 limitation, above the 95 % gate,
placeholder-covered — not re-reported.

---

## Per-Dimension Bill of Health

| Dimension | Verdict | Notes |
|---|---|---|
| 1 — Walker Byte-Accounting | Clean (1 residual edge tracked as #1822) | Cursor advances match `SptTagKind` sizes exactly; 64 KiB caps confirmed on byte count; clean EOF + non-fatal unknown-in-range bail. |
| 2 — Placeholder Fallback | Clean | `import_spt_scene` infallible; texture/size precedence, clamps, Z-up→Y-up, `-Z` winding correct + test-covered. Two new `ImportedMesh`/`ImportedNode` fields correctly default to inert for the placeholder. |
| 3 — TREE → Billboard Wiring | Clean | `Billboard` inserted on placement root (#994 holds); the new #1889 VWD marker also fires correctly for `.spt` placements (traced TREE's dual-target `index.statics`/`index.trees` fused walk); mixed `.nif`+`.spt` REFRs coexist. |
| 4 — Per-Game Variants & Route Divergence | Clean | Both routes call `parse_spt` + `import_spt_scene` identically. `detect_variant` now has two production (logging) callers — #1820 closed. `MAGIC_HEAD` exact-match test-confirmed. |
| 5 — Tag Dictionary | Clean | ~90 tags; fixed-size assignments match `format-notes.md` (now byte-accurate, #1821 closed) + the live histogram; confounders stay `Unknown`. |
| 6 — NIFAL Material Translation | Clean | Placeholder canonicalised at the single `translate_material` boundary on both routes. `is_pbr:false`, `from_bgsm:false`, `emissive_source:None`, foliage PBR overrides, two-sided alpha-test cutout all confirmed. |

The S1 placeholder contract — graceful fallback over zero rendering, never
an `Err` out of the cell loader, `.spt` REFRs routed to the SpeedTree
importer — holds end to end (live corpus run + unit suite green, matching
prior audits' numbers exactly).

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |
| **Total** | **0** |

No new findings this audit. Two of the three LOW findings carried from
the prior report (#1820, #1821) were fixed and merged on 2026-07-04 —
both verified in place. The one remaining LOW (#1822) is already filed,
open, unchanged, and correctly out of scope for re-reporting. The new
cross-cut VWD-marker wiring (#1889/#1890/#1891) that landed since the
last audit was traced through the TREE-record path specifically (the
highest-risk kind of change for this subsystem — new code touching the
shared REFR-spawn loop) and found correct: no `.spt`-specific gap.

### Suggested next step

No new issues to file. `/audit-publish` is not needed this cycle — this
report only reconfirms clean status and two closed fixes.
