# TD6-006: `PerkRecord` doc comment is stale — claims CTDA/EPFD decode are still "follow-ups"

Labels: low,tech-debt,doc-rot,documentation,esm-plugin

**Description**: `parse_perk` already fully handles per-entry CTDA (`push_ctda`) and EPFD-by-function_type decode (types 1-5), contradicting the struct doc comment's claim these are deferred follow-ups. A regression test already exercises CTDA through `parse_perk`.

**Evidence**:
`crates/plugin/src/esm/records/misc/magic.rs:220-226`.

**Impact**: A future reader trusting the stale doc comment could re-implement already-shipped CTDA/EPFD decode, or wrongly report it as a gap in a future audit.

**Related**: None named.

**Suggested Fix**: Update the doc comment to reflect that CTDA/EPFD are implemented; name any specific unhandled `function_type` values explicitly if any remain (the `_ => PerkFunctionData::None` catch-all suggests some might).



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
