# TD9-2026-10-25-05: the only operation-order test for the VATS to-hit chain is a placeholder that fails when enabled

Labels: low, test-gap, character, game:fnv, game:fo3, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6403
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 9 (`test-gap`) - **Effort**: medium (blocked on an in-game capture) - **Age**: 077ff45d6, 2026-10-24 (Refs #6350)
- **Location**: `crates/core/src/character/vats.rs:586-622` (`to_hit_chance_matches_recorded_in_game_percentages`)
- **What**: `#[ignore = "ground-truth captures pending..."]`; `GROUND_TRUTH` is an empty slice whose single fixture row is commented out, and the final
  `assert!(!GROUND_TRUTH.is_empty())` makes the test fail the moment the `#[ignore]` is removed. Its own doc says it is "the only test that can catch a wrong
  operation ORDER". `gh issue view 6350` (open) does not list the capture as an acceptance item, so nothing tracks it. It is the sole non-data-gate
  `#[ignore]` added in the window.
- **Fix**: add the capture checklist (the doc's scenario table) to #6350's acceptance list; when the captures exist, put them in the const and remove
  the attribute. Until then keep it, but it should not count as coverage. Deferred: gated on the VATS runtime milestone (#6350).


## Existing (tracked by OPEN issues; not re-filed)

- **#6223** WTHR decoders (TD2-01 of 10-21): worse, see Executive Summary.
- **#6140** `audio/lib.rs` 2086 prod LOC (+52); **#6141** `nif/import/types.rs` 2011 (flat); **#6143** eleven frame/Vulkan-driver fns with no regrowth budget (`build_pipelines_and_finish` 1404 and `build_and_upload_instances` 1322 keep growing; only `draw_frame` is pinned) — TD1-05 proposes adding the two schedule registrars.
- **#6194** the two `loading_screen.rs` TODOs (5 days old, ticketed; both strings must go when it closes).
- **#6276** new surfaces with no reference doc: `BYRO_CACHE_DIR` is now documented, but the loose `Data\` override tier still is not, and 46 of 87 distinct `"BYRO*"` literals appear in no doc. **#6277** `groundcover_bench.rs:1611` `push_bytes` doc + `skyal.md:373` memory slug (unchanged). **#6171 / #6170 / #6169 / #6168** doc-symbol rot (still live). **#6167** past-EOF refs (down 18 → 7). **#6166 / #6165** CLAUDE.md smoke-test list (20 of 40 scripts) and workspace-tree nesting (user-owned, unchanged).
- **#6350** (VATS umbrella): `VatsMode::{MenuMode, ScanMode, Playback}` never constructed and `as_u32` with no caller under `allow(dead_code)` reasons that say "constructed by the 3c lane" although 3c landed without constructing them — reword the reasons now. The ground-truth to-hit test (`vats.rs:586`) is a placeholder that fails if un-ignored and is not on the issue's acceptance list.
- **#5622** bare `allow(clippy::…)`: re-measured 32 sites by the issue's own rule; 5 not on its list (`merge.rs:758`, `graph_nodes.rs:227`, `synth_child.rs:356`, `nif_loader.rs:542`, `pre_parse.rs:692`); 10 `too_many_arguments` allows added since 4a3e89d56 pass only because a `///` parameter doc sits above them.
- **#6278** `metrics_response_mirrors_core_snapshot` guard cited by `DebugResponse::Metrics` does not exist. **#6347 / #6348** two source-scan guards scanning too narrow a region. **#6184** (closed) ratchet is per file — see TD9-03.
- Closed and verified holding: #6271 (exterior split), #6272, #6274, #6275, #6144, #6139, #6148 (residual: TD2-10).

## Deferred (gated on an in-progress milestone)

- VATS ground-truth captures for `to_hit_chance_matches_recorded_in_game_percentages` (TD9-05) and the VATS runtime dead-code reasons: gated on **#6350**.
- The 13 parsed-never-read EDID+FULL record maps (TD6-01): BSGN / HUNG-DEHY-SLPD / RCPE-RCCT / CPTH-CAMS are owed to future character-ruleset, crafting and load-screen milestones (**#6194** for the camera paths); decide per type whether to list or drop.
- Golden-frame baselines (regenerated 2026-10-22; 12 renderer/shader commits since): currency cannot be judged without a GPU — `/audit-runtime` or a local `cargo test --release -p byroredux -- --ignored golden`.
- Pre-2026-06-07 audit reports: 17 of 142 CRITICAL/HIGH blocks have no issue trace (predate `/audit-publish`); of 6 checked against code, 5 are fixed. `RL-01` ("sRGB light colours not linearised", 2026-04-12c) is a **wrong premise** under the project colour-space policy — nobody should act on it. The other 10 were not individually verified.

## Checked, clean

- **Dim 1**: `cell_loader/exterior.rs` 1687 → 210 prod LOC (split in `1d383cc5a`; family 1713 LOC, largest member 635). The 5 `match` expressions with >50 arms are exhaustive enum maps (`ui_input.rs` ×2, `Effect::kind`, `Token` Display, condition function-id dispatch); the 3 `mod.rs`/`lib.rs` with >20 `pub use` are single-job facades.
- **Dim 2**: single-home guards hold — descriptor helpers, `upload_ctx!`, `with_extension_host` (9 callers), `bytes::` (zero `from_le_bytes` in `byroredux/src` production), `paths::`, `submit_initial_layouts`, `coord.rs`, archive access via `asset_provider`, `source_scan` allowlist. `nif/blocks/dispatch.rs` (new) shows no duplicated string arms.
- **Dim 3**: `README` / `CLAUDE.md` / `contributing.md` flags and `--test` names resolve; `docs/engine/index.md` complete (82 docs); `ecs.md` stage list, `architecture.md` "35 members", `debug-cli.md` "80 components", `testing.md` "~106 dispatch tests", `shader-pipeline.md` binding table 0–25, `GpuMaterial` / `GpuLight` / `GpuRayBudget` tables, `Vertex` 104 B and `GpuWaterParams` 368 B all match code.
- **Dim 4**: 81 `/audit-x … Dim N` cross-references and all `Dim N` owner-map rows resolve; 25 "default all N" counts match; ~20 sampled numeric claims within 20%; all guards named in skills exist; `docs/audits/` 90+-day reports: 77 of 80 CRITICAL/HIGH traced to issues, the other 3 resolved in code. `ROADMAP.md` is at its 800-line cap exactly — the next growth breaches it.
- **Dims 5/6/7**: no stale marker; `todo!()` 0; every new Component/Resource/system/installer/console command since the baseline has a production writer and reader; `cxx-bridge` (26 lines) and `platform` (60 lines) placeholders unchanged; NIF version gates have zero bare literals in production; no inline Vulkan limit; no shader `#define`/`const` bypasses `shader_constants_data.rs`.
- **Dim 8**: `allow(dead_code)` — the remainder are justified (documented reserved fields, schema vocabulary, layout mirrors); `#[deprecated]` 0; `// removed:` 0; every `[features]` branch live.
- **Dim 9**: no `workspace_hygiene_tests` guard is `#[ignore]`d; all 58 pin rows name a real ignored fn; every feature has a CI lane; no commented-out assertions or smoke-only tests among the 378 tests added; the 9 closed-HIGH fixes in the window all carry hosted, non-ignored tests.

## Cleanup note

Report files under `/tmp/audit/tech-debt/` and the audit cargo target dir (`/mnt/data/tmp/td-audit-target`) are removed after this report is written. Suggested next step: `/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-10-25.md`.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD9-2026-10-25-05`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
