# Issue #1283: Runtime/visual audit dimension — extend Task 8 harness to per-game cell-load telemetry (#1277 Workstream D)

**State**: OPEN
**Labels**: enhancement, medium, tech-debt

## Body

**Child of #1277 — Workstream D (runtime/visual audit dimension).**

Extend Task 8's translation-completeness harness toward the broader **runtime/visual audit dimension** the original epic identified as the meta-cause of "audits are outdated, missing plain-sight issues" — the `audit-*` skills inspect code correctness but no audit dimension inspects rendered output or runtime behavior, so impactful Fallout symptoms surface only through manual telemetry sweeps.

## What Task 8 landed

`cargo test -p byroredux-nif --test translation_completeness -- --ignored` walks 200 NIFs per game through parse + import, collects per-game `MaterialStats`, asserts structural-consistency invariants, prints a per-game fill-rate comparison table. Diagnostic enough to surface "FNV's `m_kind%` dropped from 9.6 to 4.0 after my change" type regressions. See commit `294e68f1`.

## What's still missing

The harness is *importer-level* (parse → ImportedMesh). The broader runtime/visual audit needs to cover:

- **Cell-load runtime telemetry** — `tex.missing` / `mesh.cache` / `light.dump` counts per cell, per game. Today these are gathered by the manual headless-telemetry sweep ([docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md](../blob/main/docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md)) — should be a per-PR regression check.
- **Screenshot-diff regression** — golden frames per game's representative interior. A change that breaks FO4 visually surfaces as a screenshot delta, not just a test failure. The infra for golden frames exists ([byroredux/tests/golden_frames.rs](../blob/main/byroredux/tests/golden_frames.rs)) but only covers the cube demo — needs per-game cell-load fixtures.
- **TLAS / GpuLight / GpuMaterial census** — aggregate "what's actually in the scene" counts per cell, per game. A change that drops 30% of FO4 lights without breaking tests would be visible here.

## Concrete deliverables

- [ ] **`audit-runtime` skill** — new `.claude/commands/audit-runtime.md` orchestrator that:
  1. Launches the engine headless (Xvfb) on a per-game representative cell.
  2. Drives `byro-dbg` for `stats` / `tex.missing` / `mesh.cache failed` / `light.dump` / `bench-stats`.
  3. Compares against a baseline TSV per-cell-per-game (similar pattern to `crates/nif/tests/common/mod.rs::PerBlockHistogram::compare_histograms`).
  4. Surfaces regressions as audit findings the same way `audit-fnv` / `audit-fo4` do today.
- [ ] **Per-game representative-cell list** for the audit fixtures. Probable picks from this session's investigation:
  - Oblivion: `ICMarketDistrictTheGildedCarafe` (the existing "gorgeous baseline")
  - FNV: `GSDocMitchellHouse` (already used in FALLOUT_SYMPTOMS)
  - FO3: `MegatonPlayerHouse`
  - Skyrim SE: `WhiterunDragonsreach`
  - FO4: `InstituteBioScience`
- [ ] **Screenshot-diff harness** — extend `golden_frames.rs` (or sibling) to render per-game representative-cell views at a known camera position, encode reference PNGs, fail tests on > N% pixel delta. Gated on game-data env vars like the other ignored tests.
- [ ] **`audit-suite` orchestrator update** — add the runtime audit to the `/audit-suite` preset so it runs alongside per-game audits.

## Why this matters

This is the structural fix for the original epic's meta-complaint: "Our audits are quite outdated and thus ignoring many issues, some visible at plain sight." The 4-agent survey confirmed the renderer is genuinely clean (zero per-game branches) — the gap is that audits never *look at* what the renderer produces. This workstream makes runtime behavior auditable the same way code is.

## References

- Parent epic: #1277
- Existing harness this builds on: `crates/nif/tests/translation_completeness.rs` (#1277 Task 8, commit `294e68f1`)
- Telemetry source-of-truth: [docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md](../blob/main/docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md)
- Existing audit infra: [.claude/commands/audit-*.md](../tree/main/.claude/commands)
- Golden-frame infra to extend: [byroredux/tests/golden_frames.rs](../blob/main/byroredux/tests/golden_frames.rs)
- Histogram-baseline regression pattern: [crates/nif/tests/common/mod.rs](../blob/main/crates/nif/tests/common/mod.rs) — `PerBlockHistogram` + `compare_histograms`
