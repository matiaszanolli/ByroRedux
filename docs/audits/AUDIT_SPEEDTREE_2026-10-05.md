# SpeedTree Subsystem Audit — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: [AUDIT_SPEEDTREE_2026-09-29.md](AUDIT_SPEEDTREE_2026-09-29.md) (HEAD `9fcfdc3fc`, 313 commits ago) · **Audited**: Dimensions 1 (Walker Byte-Accounting), 3 (TREE→Billboard Wiring), 5 (Tag Dictionary, doc-only delta) and 6 (NIFAL Material Translation) · **Unchanged since baseline (skimmed)**: Dimension 2 (Placeholder Fallback; the only delta is a new test) and Dimension 4 (Per-Game Variants & Route Divergence; the only delta is an unrelated light-falloff argument in `nif_loader.rs`)

**Depth**: `deep`. The real-data corpus harness was re-run for all three games.
**Execution**: single pass, in-context, no sub-agents. The per-dimension notes are in `/tmp/audit/speedtree/dim_1.md` … `dim_6.md`, and this report is reconciled against all six.

## Commands run

| Lane | Result |
|---|---|
| `cargo test -p byroredux-spt` | 55 unit + 3 synthetic pass; 4 corpus tests ignored |
| `cargo check -p byroredux-spt --features recon --examples` | clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus, `--release --test parse_real_spt -- --ignored` (all three `BYROREDUX_*_DATA` set, `BYROREDUX_REQUIRE_GAME_DATA=1`) | FNV 10/10 **100 %**, FO3 10/10 **100 %**, Oblivion 113/113 **100.00 %** (22,092 entries). `walker_stops_on_true_tlv_boundary` now prints per-archive: FNV 10, FO3 10, OBL 113, SI 26. All of them are on the boundary with shift 0, and there are **0 EOF, 0 extract-fail, 0 parse-fail** |
| `cargo test -p byroredux --bin byroredux -- billboard parse_and_import_spt vanilla_tree tree_model tree_icon spt speedtree malformed_spt --include-ignored` (1.96.0 toolchain, game data set) | 28/28, including `vanilla_tree_models_all_resolve`, `vanilla_tree_icons_all_resolve`, the new `speedtree_gust_crest_travels_downwind` and `spt_check_tolerates_a_non_ascii_tail` |
| `cargo test -p byroredux-spt placeholder` | 9/9 |

The engine was not launched, per the suite rules. `m-trees.sh` was reviewed statically only.

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| SPT-2026-09-29-D3-01 (`m-trees.sh` lacks the debug-server opt-in, misdiagnoses the failure, has no Oblivion arm) | Folded into **#5142**, CLOSED by `858dee21f` (with `89cadf236`). Verified: `export BYRO_DEBUG_SERVER=1` is set before the launch, there is an explicit attach check, and an `obl` arm exists (Great Forest grid 0,0 r3). The new attach check has a gap of its own; see SPT-D3-01 below. |
| SPT-2026-09-29-D3-02 / **#5137** (streaming `.spt` skip byte-slices a `&str`) | CLOSED by `c83e4837a`. Verified: `is_spt_model_path` compares `&[u8]` (`byroredux/src/streaming.rs:1714-1717`). It is pinned by `spt_check_tolerates_a_non_ascii_tail`. |
| SPT-2026-09-29-D1-01 / **#5138** (post-#4122 doc rot) | CLOSED by `567d7e064`. Verified: `tail_offset`, the `FixedBytes` example, the 12002/12003 comments, `spt_tail.rs` and `stream.rs` are fixed. Two residual claims remain; see SPT-D1-01 below. |
| SPT-2026-09-29-D1-02 / **#5139** (boundary gate drops failures, prints cumulative totals) | CLOSED by `b0faf598a`. Verified: failures are counted per archive and `assert_eq!(…, 0)` runs before the boundary assertion. This run printed per-archive deltas. |
| **#4749** (overlay PBR recompute and the foliage collision) | CLOSED 2026-09-29, as the baseline recommended. |
| **#4923** item 5 (gust-travel pin) | CLOSED by `4dfe97f3e`. It added the behavioural test `speedtree_gust_crest_travels_downwind`, which passes. |

Searched for the new findings:
- **Open issues** (`/tmp/audit/issues.json`, 97 entries): `spt`, `speedtree`, `tree`, `m-trees`, `no entities`, `byro-dbg`, `smoke`, `billboard`, `harness`. No match.
- **Closed issues**: `no entities` and `m-trees`. The nearest is #5142 (CLOSED); SPT-D3-01 is a defect in that fix, not a regression of the bug it closed.

---

## Findings

### SPT-D3-01: `m-trees.sh`'s new attach check reads a real zero-billboard result as "byro-dbg never answered", so the gate's own target regression is blamed on the debug attach
- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `docs/smoke-tests/m-trees.sh:126-131`, `:133`, `:147-151`; `tools/byro-dbg/src/display.rs:10-13`; `tools/byro-dbg/src/main.rs:57`
- **Status**: NEW (a defect in the #5142 fix `858dee21f`)
- **Description**: #5142 added this guard so that a dead `byro-dbg` session is no longer parsed as 0 billboards:
  ```bash
  if ! grep -qE '^\([0-9]+ entities\)' "$dbg_log"; then
      echo "… HARD FAIL — byro-dbg never answered 'entities Billboard' … the attach itself failed"
  ```
  `byro-dbg` never prints `(0 entities)`. For an empty `EntityList` it prints `(no entities)` (`display.rs:11-13`). In piped-stdin mode that text follows the unterminated `byro> ` prompt, so the line is `byro> (no entities)`. That line matches neither the `[0-9]+` alternative nor the `^` anchor. With N > 0, the `(N entities)` row follows the entity lines on its own line, so the anchor matches; this is why the live-verified run (1416 billboards) passed.
- **Evidence**:
  - When a cell loads but the `.spt` route spawns no `Billboard` entities, the attach-failure branch fires. That is the Phase-1.5 regression this gate exists to catch, and the failure is still blamed on the attach.
  - The diagnosis written for this case is now unreachable for a zero count. That message is `zero indicates the .spt extension switch isn't routing` at `:150`, and it can only fire for 0 < n < floor.
  - `m41-equip.sh:175` uses an unanchored `awk '/^\(.*entities\)/'`, which also does not match `byro> (no entities)`. It is not in this audit's scope, but it has the same shape.
- **Impact**: The pass/fail verdict is still correct, because both branches HARD FAIL. The diagnosis is inverted, though. Before #5142, a broken attach blamed the healthy `.spt` route. Now a broken `.spt` route blames the attach, and the triager is pointed at the debug server rather than `synth_child.rs` / `parse_and_import_spt`. This is a manual gate only, with no CI exposure.
- **Related**: #5142 (CLOSED), SPT-2026-09-29-D3-01, `scripts/check-byro-dbg-harness-contracts.sh` (it checks the opt-in and screenshot shapes, not the response parsing).
- **Suggested Fix**: Treat `(no entities)` as a valid answer with a count of 0. For example, make the attach check `grep -qE '\((no|[0-9]+) entities\)'` (unanchored, which tolerates the `byro> ` prefix) and parse `no` as 0, so the existing floor branch reports the `.spt` routing diagnosis.

### SPT-D1-01: The #5138 doc sweep left two contradictions: `parse_spt` says all five fatal errors are `InvalidData` (the underflow is `UnexpectedEof`, which `stream.rs` now states), and the boundary gate's docstring names two #4122 culprits instead of three
- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting (also Tag Dictionary)
- **Location**: `crates/spt/src/parser.rs:82-90`; `crates/spt/src/stream.rs:8-11`, `:52-63`; `crates/spt/tests/parse_real_spt.rs:263-266`
- **Status**: NEW (residue of #5138's sweep; the `parse_spt` sentence dates to `57fdcc577`, 2026-08-31)
- **Description**: There are two stale claims:
  - **Error kind.** `parse_spt`'s rustdoc reads "Returns `Err(io::Error)` (`InvalidData`) on five fatal conditions: magic-header mismatch, stream underflow during a partially-read payload, …". The underflow condition comes from `SptStream::read_bytes`, which returns `io::ErrorKind::UnexpectedEof` (`stream.rs:54-55`). `567d7e064` corrected `stream.rs`'s module doc to say exactly that, so the crate's two docs now disagree about the same error.
  - **Culprit count.** The `walker_stops_on_true_tlv_boundary` docstring says "The culprits were two dictionary entries — `10002` (stride 1, now 32) and `13013` (7 bytes, now 4)". The gate itself found a third, `10003` (stride 8 → 32). `format-notes.md` records it as "Culprit 3 (found by the stronger gate)", and `tag.rs` and the SKILL list all three.
- **Evidence**: `grep -rn ErrorKind` finds no consumer that branches on the kind; the only kind assertion is the magic-mismatch test at `parser.rs:377`. Both claims are therefore documentation-only.
- **Impact**: Doc rot only. A caller that matches `InvalidData` to detect "corrupt `.spt`" would miss truncated files. The two-culprit sentence hides the entry that only the stop-word check, not the shift, could catch. That is the property a future `TAG_MAX` raise depends on.
- **Related**: #5138, #4122, #3752 (the five fatal conditions).
- **Suggested Fix**: State the kinds separately: magic, string cap, array cap and context-sensitive kind are `InvalidData`, and underflow is `UnexpectedEof`. Change "two dictionary entries" to three and add `10003` (stride 8 → 32).

---

## Dimension summary

| Dimension | Delta since 09-29 | Findings | Notes |
|---|---|---:|---|
| 1: Walker Byte-Accounting | `b0faf598a` (#5139), `567d7e064` (#5138), `4ad847a81` (clippy, test) | 1 LOW (D1-01) | No production change to `read_payload` / readers. The gate fix is verified. The `parse_rate_*` sweeps still `continue` on extract failure, but the boundary gate now asserts zero extract failures on the same archives, so this is not filed. |
| 2: Placeholder Fallback | test-only (`4dfe97f3e`) | 0 | Skimmed. 9/9 placeholder tests and 7/7 billboard-system tests pass. `import/mod.rs` is untouched. |
| 3: TREE→Billboard Wiring | `c83e4837a`, `858dee21f`, `89cadf236`, plus unrelated `streaming.rs` / `synth_child.rs` / `mesh_instance.rs` edits | 1 LOW (D3-01) | The `.spt` prefetch skip is char-safe. `CachedNifImport` synthetic defaults, the mesh-level `Billboard` insert (`mesh_instance.rs:1248`) and `SpeedTreeWind` from `cached.speedtree_wind` (`:1254-1255`) are intact. Both real-archive resolve gates pass. |
| 4: Per-Game Variants & Route Divergence | `257e973d2` (non-`.spt` light arg) | 0 | Skimmed. `detect_variant` is log-only at both sites. Both routes call `parse_spt` + `import_spt_scene` and fall back to `SptScene::default()`. |
| 5: Tag Dictionary | `567d7e064` (doc) | 0 separate | Sampled sizes match `format-notes.md`. 0 unknown tags remain in the corpus. Doc residue is folded into D1-01. |
| 6: NIFAL Material Translation | 9 commits, none reaching the placeholder | 0 | The only production change is #4912's clamp argument on the texture-only path, which the placeholder never takes. Both routes reach `translate_material`. |

**Totals**: 2 new findings: **0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW**.

## Summary

All four findings from the 09-29 baseline were fixed and closed within a week (#5137, #5138, #5139 and #5142), and #4749 is closed. The walker holds its best measured state: 100 % unknown-tag-clean in all three games, and 159/159 files stop on the true TLV boundary. The boundary gate now proves it saw every file (0 extract failures, 0 parse failures) and reports per-archive counts.

Both new findings are LOW and come from those fixes. The `m-trees.sh` attach check reverses the old misdiagnosis: a real zero-billboard regression is now blamed on the debug attach (D3-01). The doc sweep left the `parse_spt` error-kind sentence contradicting the `stream.rs` doc it had just corrected, and left the gate's docstring missing the third #4122 culprit (D1-01). No production SpeedTree code changed in this window apart from the char-safe prefetch test.

### Suggested next step

`/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-10-05.md`

Labels:
- SPT-D3-01: `low`, `bug`, `speedtree`, `tech-debt` (smoke harness; no dedicated label), `game:fnv` + `game:fo3` + `game:oblivion`.
- SPT-D1-01: `low`, `documentation`, `speedtree`, `doc-rot`.
