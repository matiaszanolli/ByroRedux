# SpeedTree Subsystem Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: [AUDIT_SPEEDTREE_2026-09-22.md](AUDIT_SPEEDTREE_2026-09-22.md) (HEAD `ee6d3fb39`, 272 commits ago) · **Audited**: Dimensions 1–6 (every dimension had at least one commit on its `Paths:` since the baseline) · **Unchanged since baseline (skimmed)**: no whole dimension; the unchanged files were guard spot-checked only: `crates/spt/src/{stream,version}.rs`, `crates/spt/src/import/mod.rs`, `crates/plugin/src/esm/records/tree.rs`, `crates/core/src/ecs/components/billboard.rs`, `references/import_tests.rs`

**Depth**: `deep`. The real-data corpus harness was re-run for all three games.
**Execution**: single pass, in-context, no sub-agents. The notes for each dimension are in `/tmp/audit/speedtree/dim_1.md` … `dim_6.md`, and this report is reconciled against all six.

## Commands run

| Lane | Result |
|---|---|
| `cargo test -p byroredux-spt` | 55 unit + 3 synthetic pass (+1 vs baseline: `array_payload_tags`); 4 corpus tests ignored |
| `cargo check -p byroredux-spt --features recon --examples` | clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus, `--release --test parse_real_spt -- --ignored` (all three `BYROREDUX_*_DATA` set, `BYROREDUX_REQUIRE_GAME_DATA=1`) | FNV 10/10 **100 %**, FO3 10/10 **100 %**, Oblivion 113/113 **100.00 %** (was 96.46 %: #4122's `13013` fix cleared the four "tag 768" outliers), 22,092 Oblivion entries; `walker_stops_on_true_tlv_boundary` **159/159 on boundary, 159/159 shift-0, 0 EOF** |
| `cargo test -p byroredux --bin byroredux -- billboard parse_and_import_spt vanilla_tree tree_model tree_icon spt speedtree malformed_spt` | 25/25, including the two real-archive gates `vanilla_tree_models_all_resolve` and `vanilla_tree_icons_all_resolve` |
| `cargo test -p byroredux --bin byroredux -- overlay_pbr_divergence_tests` | 4/4 |

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| **#4749** (SPT-2026-09-22-D6-01, HIGH): the #4229 overlay recompute reopens the #1819 foliage collision | **Fixed in code, but the issue is still OPEN.** `2f8538334` (2026-09-26) carries a message about audio and combat and never mentions the issue. It added `ImportedMaterial::pbr_classified_at_import` (`crates/nif/src/import/types.rs:725-727`) and set it only in the NIF keyword classifier (`crates/nif/src/import/material/mod.rs:1567`). It also gated `recomputed_pbr` on the flag (`byroredux/src/material_translate.rs:607-610`). This is the fix the 09-22 report suggested. It is pinned by `overlay_swap_preserves_direct_non_keyword_pbr_overrides`. **Close #4749 citing `2f8538334`.** Today's NIFAL report lists #4749 as "unchanged", which is wrong. |
| **#4122** (tail_offset desync) | CLOSED 2026-09-24 by `9fcbee478`. Verified: corpus gate 159/159. It left doc rot behind (SPT-D1-01). |
| **#4729** (WindField frame, from the EXT audit) | CLOSED by `21430f45e`. The math is verified correct in Dim 2. The unpinned gust-travel term is **Existing #4923** item 5, so it is not re-filed. |
| EXTERIOR 2026-09-29 routing: "`m-trees.sh` lacks `BYRO_DEBUG_SERVER=1`" → `/audit-speedtree` | Confirmed and filed below as **SPT-D3-01**. It is the SpeedTree instance of EXT-D7-2026-09-29-01. |

The new findings' keywords were searched in open issues (`/tmp/audit/issues.json`), in closed issues (`m-trees`, `BYRO_DEBUG_SERVER`, `char boundary`, `SptParseError`, `walker_stops_on_true_tlv_boundary`, `tail_offset`, `13013`) and in today's sibling reports. Nothing matched. The nearest hit is #3391 (CLOSED), the same char-boundary class at a different site.

---

## Findings

### SPT-D3-01: The SpeedTree end-to-end smoke gate `m-trees.sh` is permanently red since `63c0aee3b`: the release engine never opts the debug server in, and the failure is misdiagnosed as a broken `.spt` route
- **Severity**: MEDIUM
- **Dimension**: TREE→Billboard Wiring
- **Location**: `docs/smoke-tests/m-trees.sh:62-66`, `:83-87`, `:108-109`, `:123-128`; `byroredux/src/main.rs:78-80`, `:1022`
- **Status**: NEW. This is the SpeedTree instance of EXT-D7-2026-09-29-01, which `AUDIT_EXTERIOR_2026-09-29.md` routed to this audit.
- **Description**: `63c0aee3b` (2026-09-27) made the release debug server opt-in: `debug_server_allowed(false, None)` is false unless `BYRO_DEBUG_SERVER=1` is set. `m-trees.sh` launches `cargo run --release … --bench-hold` without that variable, so the chain fails like this:
  1. `byro-dbg` cannot attach.
  2. The heredoc ends in `|| true`, which swallows the connection failure.
  3. The `(N entities)` grep finds nothing and falls back to `echo 0`.
  4. The FNV and FO3 arms both HARD FAIL on `Billboard entities=0 < floor`.
  5. The script reports "zero indicates the .spt extension switch isn't routing — see Phase 1.5".
- **Evidence**: `main.rs:79` is `debug_build || matches!(opt_in, Some("1" | "true" | "yes"))`. `m-trees.sh:62` is `cargo run --release --quiet -- "$@" --bench-frames … --bench-hold` with no environment prefix. `m-exteriors.sh:293` and `w1-water-traversal.sh` (`db8351587`) already carry the opt-in.
- **Impact**: The only captured-runtime gate for SpeedTree cannot pass. When someone runs it, it points them at the `.spt` dispatch, which is healthy (Dim 3: all wiring tests pass).
  - It is a manual gate, not in CI, so nothing noticed.
  - The harness has no Oblivion arm (`[fnv|fo3|all]`). Oblivion holds 113 of the 133 vanilla `.spt` files and 142 TREE records, and it is the only game sized by the BNAM tier. Even when fixed, the gate never exercises it.
- **Related**: EXT-D7-2026-09-29-01, `63c0aee3b`, and the same pattern in the m34, m43, m47 and m48 harnesses (routed by the EXT report).
- **Suggested Fix**:
  - Prefix the engine launch with `BYRO_DEBUG_SERVER=1 BYRO_DEBUG_PORT="$PORT"`.
  - Fail loudly when `byro-dbg` output lacks the `entities` response, instead of parsing it as 0.
  - Add an `obl` arm, for example a Cyrodiil exterior grid with `Oblivion - Meshes.bsa` and `Oblivion - Textures - Compressed.bsa`.

### SPT-D3-02: The streaming prefetch's `.spt` skip byte-slices a `&str`, so a non-ASCII MODL tail panics the cell's pre-parse
- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `byroredux/src/streaming.rs:1795-1796` (`PreflightDecision::SkipSpt`); input decode is at `crates/plugin/src/esm/records/common.rs:141-171`
- **Status**: NEW. Introduced by `a2aae52a7` (#4207, 2026-09-21), which predates the 09-22 baseline HEAD; that run missed it.
- **Description**: `model_path[model_path.len() - 4..].eq_ignore_ascii_case(".spt")` slices by byte index.
  - MODL strings come from `read_mesh_path` → `read_zstring` → `String::from_utf8_lossy`. That path rejects ASCII control bytes but accepts non-ASCII.
  - A cp1252 byte decodes to U+FFFD (3 bytes), and mod content can author UTF-8 paths. Either way, a model path whose 4th-from-last byte lies inside a multi-byte character (for example `x\u{FFFD}ab`) panics with "byte index is not a char boundary".
  - Every other `.spt` test in the tree is char-safe: `to_ascii_lowercase().ends_with(".spt")` in `synth_child.rs:544` and `nif_loader.rs:464`, and `rsplit('.')` in `nif_loader.rs:262`.
- **Evidence**: `let d = if model_path.len() >= 4 && model_path[model_path.len() - 4..].eq_ignore_ascii_case(".spt")`. The `catch_unwind` at `streaming.rs:1304` logs "panic in pre_parse_cell … recovered with empty payload (#854)".
- **Impact**: A malformed or non-ASCII mod MODL on any REFR throws away the whole cell's worker pre-parse. The cell then loads on the main-thread sync path, which costs a hitch, and a panic line is logged. There is no crash and no data loss. Vanilla paths all end in an ASCII extension and are unaffected.
- **Related**: #3391 (CLOSED, same char-boundary class in `canonical_mesh_path`), #4207, #3735.
- **Suggested Fix**: Test the bytes instead: `model_path.as_bytes().len() >= 4 && model_path.as_bytes()[model_path.len() - 4..].eq_ignore_ascii_case(b".spt")`. Add a unit case with a trailing U+FFFD.

### SPT-D1-01: #4122 left the crate's own docs asserting the desync it fixed: `SptScene::tail_offset` still says 46 % of files stop mid-payload, `tag.rs` says 13013 is 7 bytes, and 12002/12003 are still "unevidenced"
- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting (also Tag Dictionary)
- **Location**:
  - `crates/spt/src/scene.rs:117-123`
  - `crates/spt/src/tag.rs:29`
  - `crates/spt/src/tag.rs:139-147`
  - `crates/spt/docs/format-notes.md:431-437`
  - `crates/spt/examples/spt_tail.rs:289-291`
  - `crates/spt/src/stream.rs:8-10`
- **Status**: NEW
- **Description**: `9fcbee478` fixed the three mis-sized entries and updated the dispatch arms, but several other claims were not updated:
  - The public rustdoc on `SptScene::tail_offset` still lists "In 46 % of files the resync needs a 1-3 byte shift, meaning the walker stopped *inside* a payload it mis-sized". It closes with "Treat it as 'where parsing gave up'". The gate this commit added now measures 0/159 shifted and 159/159 stopping on a 14 000-band tail tag, so `tail_offset` is the true TLV boundary at `TAG_MAX`.
  - The `SptTagKind::FixedBytes` doc gives "tag `13013` = 7 bytes"; the value is 4.
  - The 12002 and 12003 arms still say "Size only … no recorded corpus evidence". `format-notes.md:431-437` repeats that claim, while the same file's 2026-09-24 entry (about lines 921-923) records that both "decode cleanly to their next tags — the two FixedBytes sizes that had no recorded corpus observation now have one".
  - `spt_tail.rs` describes the 46 % stop in the present tense.
  - Separately, and predating the baseline, the `stream.rs` module doc says errors surface as `Err(SptParseError::Truncated)`. No such type exists; the parser returns `io::Error`.
- **Evidence**: The corpus run in this audit gives `[SI] 159 files | 159 on boundary | 159 shift-0`. `grep -rn SptParseError` has exactly one hit, the doc line.
- **Impact**: A consumer reading the public API doc would treat `tail_offset` as untrustworthy, although it is now the exact precondition point #3808 named for raising `TAG_MAX`. The false 7-byte and "unevidenced" claims invite a re-litigation like the 2026-07-04 "768" dispute.
- **Related**: #4122, #3535 (12002/12003 evidence), #4120 (the prior "sweep missed a file" doc-rot pattern).
- **Suggested Fix**:
  - Rewrite the `tail_offset` bullets to state the post-#4122 measurement: the stop is the true boundary, and the tail tags start at 14 000.
  - Change the `FixedBytes` example to 4 bytes.
  - Replace the 12002/12003 caveats in `tag.rs` and `format-notes.md` with a pointer to the 2026-09-24 side confirmation.
  - Mark the `spt_tail.rs` sentence as pre-#4122.
  - Replace `SptParseError::Truncated` with `io::Error` (`UnexpectedEof`).

### SPT-D1-02: The #4122 boundary gate silently drops files that fail to parse, and prints cumulative totals under per-game labels
- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting
- **Location**: `crates/spt/tests/parse_real_spt.rs:295-303`, `:331-334`
- **Status**: NEW
- **Description**: `walker_stops_on_true_tlv_boundary` runs `let Ok(bytes) = archive.extract(path) else { continue; }; let Ok(scene) = parse_spt(&bytes) else { continue; };` before `totals.total_files += 1`, so a fatal `parse_spt` error removes the file from the denominator instead of failing the gate. The SKILL rates a new fatal path as HIGH: a dictionary edit that turns a mis-size into an array-cap, string-cap or underflow error.
  - The `parse_rate_*` tests do count errors, but against a 95 % bar that tolerates about 5 fatal Oblivion files.
  - The 26 Shivering Isles files are only in the boundary gate.
  - The per-game `eprintln!` prints the running `totals` (`[FO3] 20 files`, `[OBL] 133 files`), although FO3 has 10 files and Oblivion 113.
- **Evidence**: This run's output is `[FNV] 10 … [FO3] 20 … [OBL] 133 … [SI] 159`.
- **Impact**: This is a test gap only; production is correct today. The next dictionary expansion past `TAG_MAX`, which #4122 unblocked, is exactly the change that could introduce a fatal path this gate would hide.
- **Related**: #4122, #3752 (the five fatal conditions).
- **Suggested Fix**:
  - Count extract and parse failures, and assert they are zero, or at least report them.
  - Print per-archive deltas instead of running totals.

---

## Dimension summary

| Dimension | Delta since 09-22 | Findings | Notes |
|---|---|---:|---|
| 1: Walker Byte-Accounting | `9fcbee478` (#4122) | 2 LOW (D1-01, D1-02) | `read_payload` sizes, the u64-saturating array cap (stride 32 ⇒ count ≤ 2048), the 13005 EOF/empty-candidate arms, the five fatal errors, and LE-only reads are all verified. `best_resync_shift` is bounds-safe and diagnostic-only. |
| 2: Placeholder Fallback | `21430f45e` (#4729) on `billboard.rs`; `import/mod.rs` unchanged | 0 | The new bend axis `(d.y,0,−d.x)` tips the crown downwind, the mean lean is ≥ 0.3·bend, and the wave travels with the wind. `BsRotateAboutUp` stays yaw-only, including the 180° singularity (glam 0.30.10 `any_orthonormal_vector(−Z)` = −Y). The gust-travel pin is Existing #4923. |
| 3: TREE→Billboard Wiring | synth_child 3, import 2, registry 5, mesh_instance 9, streaming 12 commits | 1 MEDIUM (D3-01), 1 LOW (D3-02) | The `.spt` dispatch, `spt_cache_key`, `parse_and_import_spt` defaults (new `beam_volumes` inert), mesh-level `Billboard` + `SpeedTreeWind`, and the prefetch skip are all intact. |
| 4: Per-Game Variants & Route Divergence | `nif_loader.rs` cache refactor | 0 | `detect_variant` is still log-only at both sites. The loose `.spt` route cannot store a negative cache entry. `peek_or_parse_scene` (no `.spt` support) is NPC-only. |
| 5: Tag Dictionary | `9fcbee478` | 0 separate (doc rot folded into D1-01) | All sampled sizes match `format-notes.md`. The confounders stay `Unknown`, and 0 unknown tags remain in the corpus. |
| 6: NIFAL Material Translation | 13 commits on `material_translate.rs` | 0 | #4749 is fixed by `2f8538334`. The placeholder carries `pbr_classified_at_import = false`, and no other translate change reaches it. |

**Totals**: 4 new findings: **0 CRITICAL, 0 HIGH, 1 MEDIUM, 3 LOW**. One OPEN issue (#4749) is verified fixed in code and should be closed.

## Summary

The walker is now in its best measured state. #4122 put all 159 corpus files on the true TLV boundary, and all three games parse with zero unknown tags (Oblivion went from 96.46 % to 100 %). What #4122 left behind is prose: the crate's public docs still describe the desync it removed (D1-01). Its new acceptance gate would also hide the most dangerous class of regression, a new fatal parse path, behind a skipped file (D1-02). Both matter because #4122 unblocked the next step, dictionarying past `TAG_MAX`, and that step will lean on these docs and this gate.

On the engine side, the SpeedTree wiring is unchanged and green. Its one runtime gate, `m-trees.sh`, has been dead since the release debug-server opt-in landed, and it blames the healthy `.spt` route (D3-01). A small char-boundary panic sits in the streaming prefetch's `.spt` skip (D3-02). Last cycle's HIGH (#4749) was fixed three days later inside an unrelated-looking commit, and its issue was never closed.

### Suggested next step

`/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-09-29.md`

Labels:
- SPT-D3-01: `medium`, `bug`, `speedtree`, `tech-debt` (smoke harness), `game:fnv` + `game:fo3`. Note the missing Oblivion arm.
- SPT-D3-02: `low`, `bug`, `speedtree`, `terrain-exterior`.
- SPT-D1-01: `low`, `documentation`, `speedtree`, `doc-rot`.
- SPT-D1-02: `low`, `enhancement`, `speedtree`, `test-gap`.

Also close #4749 with a pointer to `2f8538334`. That is a manual step; it is not part of publishing.
