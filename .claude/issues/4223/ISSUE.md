# TD4-001: `audit-esm/SKILL.md` backticks a test helper that no longer exists — the paragraph already names its replacement

Labels: low,tech-debt,doc-rot,documentation

**Description**: `record_parsers_with_embedded_form_ids_take_a_remap` no longer exists (`48acaea2` inverted the guard to `all_record_parsers()`); the same paragraph already narrates the rename two sentences later.

**Evidence**:
`.claude/commands/audit-esm/SKILL.md:276`.

**Impact**: A future auditor grepping for the backticked (asserted-live) name finds nothing and could wrongly conclude the guard was removed entirely, rather than renamed.

**Related**: None named.

**Suggested Fix**: Replace the backticked dead name with italics or the current `all_record_parsers()` name.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
