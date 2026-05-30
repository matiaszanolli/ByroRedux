**Severity:** MEDIUM · **Dimension:** Coverage (test infrastructure) · **Game Affected:** All

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-04).

## Description
The translation-completeness harness measures whether parsed blocks **translate** to a canonical Material — it does **not** catch parse-block truncation (the truncated collision blocks in the Oblivion HIGH finding never produced a Material anyway). No test asserts `parsed_block_count == header.num_blocks` (Oblivion) or a NiUnknown-rate ceiling (sized games). That is why the 3 Oblivion truncations are invisible to `cargo test` and only surfaced via an ad-hoc probe.

## Location
- `crates/nif/tests/translation_completeness.rs::cross_game_translation_completeness` (material-translation surface)
- `crates/nif/tests/per_block_baselines.rs` (candidate home for the new pin)

## Evidence
`translation_completeness.rs:198-235` iterates 5 games measuring material fields and `structurally_inconsistent`, with no block-count parity assertion. `d5_coverage`'s `parse_nif ok` metric counts truncation-recovered files as Ok. Both test files confirmed present.

## Impact
Coverage regressions of the sizeless-cascade class (e.g. the Oblivion `bhkConvexSweepShape`/`bhkMeshShape` truncation) land silently; only an ad-hoc probe detects them.

## Suggested Fix
Add a per-game pin (extend `per_block_baselines.rs` or a new `block_coverage_baselines.rs`) that, over a sampled set of vanilla NIFs per game, asserts `parsed_blocks == header.num_blocks` (Oblivion) and a NiUnknown-rate ceiling (sized games).

## Related
#568 (nif_stats clean metric counted NiUnknown-recovered as clean — CLOSED, same blind spot), #601. **Distinct from #1320** (OPEN) — that issue is about adding *value-level fill-rate thresholds* to `translation_completeness`; this finding is about a *different surface* (parse-block / block-count parity), which `translation_completeness` is explicitly the wrong place for.

## Completeness Checks
- [ ] **SIBLING**: Pin covers all 7 games, with Oblivion (sizeless → exact parity) handled distinctly from sized games (NiUnknown ceiling)
- [ ] **CANONICAL-BOUNDARY**: Keep this pin separate from `translation_completeness` (material-translation surface) — do not conflate parse-block parity with Material fill-rate
- [ ] **TESTS**: The new pin fails on the current `handscythe01.nif`/`oar01.nif`/`ungrdltraphingedoor.nif` truncation (red before the HIGH fix, green after)
