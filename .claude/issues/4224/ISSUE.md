# TD4-003: `audit-fnv/SKILL.md`'s own historical `boot.rs` mention is backticked, contrary to the gate's advisory convention

Labels: low,tech-debt,doc-rot,documentation

**Description**: Prose is correct ("the former `boot.rs`") but still backticks a name that resolves nowhere in the tree; the path-gate's convention is italics for historical/dead names, not backticks.

**Evidence**:
`.claude/commands/audit-fnv/SKILL.md:188`.

**Impact**: Purely a convention-compliance nit flagged by `_audit-validate.sh`'s advisory (non-fatal) symbol list.

**Related**: None named.

**Suggested Fix**: Change to italics: "the former *boot.rs*".



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
