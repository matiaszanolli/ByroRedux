# FNV-2026-10-18-11: Skill text asks for "`stats` returns to baseline", but `stats` Entities is a never-reused spawn counter

- **Issue**: #6060 (https://github.com/matiaszanolli/ByroRedux/issues/6060)
- **Labels**: low, documentation, doc-rot, tech-debt, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D1-07. **Status**: NEW (audit-tooling doc rot).
- **Location**: `.claude/commands/audit-fnv/SKILL.md` Dimension 1; `byroredux/src/app_events.rs:853` (`stats.entity_count = world.next_entity_id()`), `crates/core/src/ecs/world.rs:415-420`.
- **Evidence**: Entities went 2,164 → 106,749 → 108,911 across the trip while every live-state owner returned exactly (round-trip table above).
- **Suggested Fix**: replace the criterion with the owner set used in this audit (named entities from `entities`, `phys.stats`, `rt.integrity`, `skin.coverage`, `mesh.cache`), or add a live-entity count to `stats`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

