# #4355 — TD3-007: The Skyrim CHARAL ruleset was wired by #3848, but feature-matrix, charal.md and the corpus-test comments still say "unwired"

**Labels**: low, character, game:skyrim, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4355

- **Severity**: LOW · **Dimension**: 3
- **Location**: `docs/feature-matrix.md:251,259-262,330`, `docs/engine/charal.md:352`, `crates/plugin/tests/parse_real_esm.rs:382-383,490-491` · **Status**: NEW · **Age**: `e13985dfc` (09-12) · **Effort**: small · **Kind**: doc-rot · **Related**: TD9-001 (the assertion half)
- **Suggested Fix**: Mark the Skyrim SE cell ✓, rewrite the gap prose to cover Oblivion only, and drop the #3170 aside.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
