# SpeedTree Subsystem Audit — 2026-07-03

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker + placeholder-billboard import fallback (Session 33 Phase 1, "S1"),
plus its cross-cut wiring in `byroredux/src/cell_loader/references.rs`,
`byroredux/src/cell_loader/spawn.rs`, `byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`, and
`byroredux/src/systems/billboard.rs`.

**Depth**: `deep` — corpus acceptance harness run live against on-disk
FNV / FO3 / Oblivion BSAs; full crate unit + integration suite run;
every carried-forward finding from `AUDIT_SPEEDTREE_2026-07-02.md`
re-verified against current source and against `gh issue list`.

**Method**: Read the full crate (`parser.rs`, `stream.rs`, `tag.rs`,
`scene.rs`, `version.rs`, `import/mod.rs`) plus the five cross-cut wiring
files, ran the unit suite and the `--ignored` corpus gate, cross-checked
every 2026-07-02 finding's issue number against `gh issue list --state all`,
and diffed `git log` since 2026-07-02 across every file in scope to confirm
nothing relevant changed underneath the prior audit's conclusions.

---

## Verification runs (this audit)

### Corpus acceptance gate — live run

```
BYROREDUX_FNV_DATA=… BYROREDUX_FO3_DATA=… BYROREDUX_OBL_DATA=… \
  cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture
```

```
[FNV] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[FO3] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[OBL] 113 files | 113 with entries| 4 hit unknown tag | 20425 entries  | 96.46 % coverage
  unknown-tag samples:
    trees\shrubms14boxwood.spt         | tag=768 (0x0300) at offset 4507
    trees\treecottonwoodsu.spt         | tag=768 (0x0300) at offset 5641
    trees\treems14canvasfreesu.spt     | tag=768 (0x0300) at offset 6211
    trees\treems14willowoakyoungsu.spt | tag=768 (0x0300) at offset 5946
```

Identical to the 2026-07-02 run byte-for-byte (same files, same offsets,
same coverage rates). All three gates clear the ≥ 95 % floor.

### Unit + integration suite

`cargo test -p byroredux-spt --release` — 46 unit tests + 3 synthetic
integration tests, all pass, 0 failures. `parse_synthetic_spt.rs`'s
byte-pinned regression fixture (#998) still round-trips.

---

## Dedup pass (mandatory)

`gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json
number,title,state,labels --search "speedtree OR spt OR TREE"` plus a
targeted `--search "SPT-NEW"` (state: all) confirms every finding carried
forward from `AUDIT_SPEEDTREE_2026-07-02.md` is now tracked:

| 07-02 finding | Issue | State | This audit |
|---|---|---|---|
| SPT-NEW-05 (HIGH, foliage keyword collision) | #1819 | **CLOSED** | Fix verified in place (below) |
| SPT-NEW-01 (LOW, `detect_variant` dead code) | #1820 | OPEN | Re-verified, unchanged — skip |
| SPT-NEW-06 (LOW, format-notes.md byte-offset doc nit) | #1821 | OPEN | Re-verified, unchanged — skip |
| SPT-NEW-07 (LOW, 13005-before-tail String misparse edge) | #1822 | OPEN | Re-verified, unchanged — skip |

No new GitHub issues reference `spt`/SpeedTree/TREE content since
2026-07-02 beyond these four. No stale-premise risk found on any of them.

### #1819 (SPT-NEW-05) — fix verification

`git log` shows commit `1748e148` ("Fix #1819: classify SpeedTree
placeholder billboard PBR at import time", 2026-07-03) landed *after*
the 2026-07-02 report and *on the same day* as this audit. Read
`crates/spt/src/import/mod.rs:342-343` directly:

```rust
metalness_override: Some(0.0),
roughness_override: Some(0.85),
```

`placeholder_billboard_mesh` no longer leaves both overrides `None`, so
`translate_material` never falls through to `classify_pbr_keyword`'s
texture-path substring classifier for SpeedTree content — closing the
Boxwood→WOOD / Elderberry→GLASS collision the prior audit found. A
regression test landed alongside the fix:
`placeholder_billboard_sets_foliage_pbr_overrides_regardless_of_texture_path`
(`import/mod.rs:548-571`) exercises exactly the three colliding leaf paths
(`shrubboxwoodleaves`, `shrubgenericelderberryleavesfa`, a generic control)
and asserts `metalness_override == Some(0.0)` /
`roughness_override == Some(0.85)` on all three. Ran green in this audit's
`cargo test` pass. **Confirmed fixed, not a regression.**

The issue's own "Completeness Checks" included a SIBLING item ("full
FO3/FNV `.spt`-backed foliage texture corpus… scanned for other keyword-
substring collisions before closing") that reads unchecked in the issue
body. Because the fix is a blanket override (bypasses the classifier
entirely for every `.spt` leaf path, not a per-species patch), the
specific SIBLING scan is moot — no texture path can reach the classifier
via this route anymore regardless of what other collisions exist in the
corpus. Not re-flagging; noting only for completeness.

---

## Fresh dimension sweep

Walked all six dimensions directly against current source (no reliance
on the prior report's text) to check for anything six consecutive audits
missed:

- **Dimension 1 (Walker Byte-Accounting)**: `parser.rs` unchanged since
  2026-06-09. Cursor accounting, `MaybeStringElseBare`, EOF/out-of-range
  bail, and the 64 KiB caps (checked against byte count, not raw count)
  all read exactly as documented. No new fatal-error path introduced.
- **Dimension 2 (Placeholder Fallback)**: `import_spt_scene` is still
  infallible; size precedence (OBND→BNAM→MODB→default, `[16,8192]`
  clamp), `-Z` normal/winding convention, and the Z-up→Y-up `bs_bound`
  swap via the canonical `zup_to_yup_pos` helper are all unchanged and
  test-covered.
- **Dimension 3 (TREE→Billboard Wiring)**: `parse_and_import_spt`
  (`references.rs:1304-1420`) unchanged since 2026-06-09; synthetic
  defaults (`bsx_flags=0`, `root_flags=0`, `flame_attach_offset=None`,
  `attach_points=None`) all still explicit and commented with their
  originating issue numbers. `spawn.rs` (unchanged since 2026-06-19)
  still inserts `Billboard` from `placement_root_billboard`.
- **Dimension 4 (Per-Game Variants)**: `detect_variant` still has zero
  production consumers (only its own tests + the recon-gated
  `spt_dissect.rs` example) — this is SPT-NEW-01/#1820, still open,
  not re-reported. `MAGIC_HEAD` exact-match still enforced by
  `bytes.starts_with(...)` in `parse_spt`, independent of `detect_variant`.
- **Dimension 5 (Tag Dictionary)**: Unit tests
  (`fixed_byte_payload_tags`, `string_payload_tags`, `u32_payload_tags`,
  `vec3_payload_tags`, `unknown_for_out_of_dictionary_tags`,
  `tag_13005_is_maybe_string_else_bare`) all still pass; no dictionary
  edits since the last audit.
- **Dimension 6 (NIFAL Material Translation)**: the #1819 fix (above) is
  the only change in this dimension. `is_pbr:false`, `from_bgsm:false`,
  `emissive_source:None`, two-sided alpha-test cutout, and the single
  `translate_material` boundary (no parallel spt-material path) all still
  hold on both the cell-loader route and the `--tree` loose route.

`git log --since=2026-07-02` across every in-scope file confirms the only
relevant commit in the window is `1748e148` (the #1819 fix itself); the
one other commit touching a cross-cut file
(`5707419b`, "time the NPC-spawn calls in `load_references`") only adds
wall-clock instrumentation around the ACHR/NPC spawn dispatch arms and
does not touch the `is_spt` branch or `parse_and_import_spt`.

No new findings surfaced. The subsystem's bill of health this audit is
**clean** at every dimension except the three already-filed, already-
tracked LOW tech-debt items, which stay open and unchanged.

---

## Findings

None — all findings from the prior audit lineage are either fixed and
verified (SPT-NEW-05 / #1819) or already tracked as open issues
(SPT-NEW-01 / #1820, SPT-NEW-06 / #1821, SPT-NEW-07 / #1822). No new
issues were found in this sweep.

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
| SPT-NEW-05 (foliage keyword collision) | #1819 | **Newly CLOSED this window** — `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` at `import/mod.rs:342-343`; regression test `placeholder_billboard_sets_foliage_pbr_overrides_regardless_of_texture_path` passes |

The **14000-band Oblivion tail** (4 files bail at tag `768`) remains the
documented `format-notes.md` Phase-1 limitation, above the 95 % gate,
placeholder-covered — not re-reported (its doc-precision nit is tracked
separately as #1821/SPT-NEW-06).

---

## Per-Dimension Bill of Health

| Dimension | Verdict | Notes |
|---|---|---|
| 1 — Walker Byte-Accounting | Clean (1 residual edge tracked as #1822) | Cursor advances match `SptTagKind` sizes exactly; 64 KiB caps confirmed on byte count; clean EOF + non-fatal unknown-in-range bail; only `Err` paths funnel to the caller's graceful `warn→None→skip-REFR`. |
| 2 — Placeholder Fallback | Clean | `import_spt_scene` is infallible; texture/size precedence, clamps, Z-up→Y-up, `-Z` winding all correct + test-covered. |
| 3 — TREE → Billboard Wiring | Clean | `Billboard` inserted on placement root (#994 holds); synthetic defaults honoured; mixed `.nif`+`.spt` REFRs coexist; no BLAS/entity leaks on cell unload. |
| 4 — Per-Game Variants & Route Divergence | Mostly clean | Both routes call `parse_spt` + `import_spt_scene` identically; loose route's `default()` params documented. `detect_variant` still dead outside its own tests (#1820, open, not re-reported). `MAGIC_HEAD` exact-match test-confirmed. |
| 5 — Tag Dictionary | Clean | ~90 tags; fixed-size assignments match `format-notes.md` + the live histogram; confounders stay `Unknown`; 4 Oblivion bails traced to the documented 14000-band limitation (#1821, open, not re-reported). |
| 6 — NIFAL Material Translation | **Clean — prior HIGH now closed** | Placeholder canonicalised at the single `translate_material` boundary on both routes. `is_pbr:false`, `from_bgsm:false`, `emissive_source:None`, two-sided alpha-test cutout all confirmed. #1819 fix verified in place with passing regression test — the foliage-keyword-collision HIGH from the last two audits is resolved. |

The S1 placeholder contract — graceful fallback over zero rendering, never
an `Err` out of the cell loader, `.spt` REFRs routed to the SpeedTree
importer — holds end to end (live corpus run + unit suite green, matching
2026-07-02's numbers exactly).

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |
| **Total** | **0** |

No new findings this audit. The one HIGH carried across the prior three
reports (SPT-NEW-05 / #1819, foliage PBR keyword-collision) was fixed and
merged the same day as this audit (`1748e148`) — verified in place with a
passing regression test. The three remaining LOW findings (#1820, #1821,
#1822) are already filed, open, unchanged, and correctly out of scope for
re-reporting per the dedup protocol.

### Suggested next step

No new issues to file. `/audit-publish` is not needed this cycle — this
report only reconfirms clean status and one closed fix.
