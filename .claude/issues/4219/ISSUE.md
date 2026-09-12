# TD1-003: `crates/plugin/src/esm/records/mod.rs` mixes a re-export barrel with a 435-line parsing entry point

Labels: low,tech-debt,esm-plugin,bug

**Description**: The file legitimately barrels 21 `pub use` re-exports from already-split submodules, but also owns `parse_esm`, `character_rules_profile`, and `parse_esm_with_load_order` (confirmed 435 lines) — the latter mixing setup/per-GRUP-dispatch/assembly in one body via a 19-arm top-level GRUP match (below the 50-arm table trigger; each arm's FO4-gating differs too much for a table to help).

**Evidence**:
`crates/plugin/src/esm/records/mod.rs:185-619`.

**Impact**: No runtime impact — pure maintainability.

**Related**: None named.

**Suggested Fix**: Split the parsing entry point out to a sibling `records/parse.rs`, re-exported once from `mod.rs` to restore it as a pure barrel; independently extract the setup phase and heaviest GRUP arms (`LTEX`, `SCOL`, `PKIN`/`MOVS`/`MSWP`) into named helpers, matching how CELL/WRLD already delegate to `cell`'s walker.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
