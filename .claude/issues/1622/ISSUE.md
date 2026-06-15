# RT-5: Runtime-baseline README schema lists metric keys no committed TSV uses

- **GitHub**: #1622
- **Severity**: low
- **Labels**: low, tech-debt, documentation
- **Source**: docs/audits/AUDIT_RUNTIME_2026-06-14.md (RT-5)

## Description
`.claude/audit-baselines/runtime/README.md:24-34` schema block lists `tex_missing_entity_count` (L28), `light_count_point` (L31), `bench_draw_calls_total` (L33) — keys in no committed TSV and not in the skill's Phase 3 contract. README documents a contract the skill contradicts.

## Evidence
README schema vs `fnv-FreesideAtomicWrangler.tsv` / `fo4-InstituteBioScience.tsv` (neither carries those keys); SKILL.md Phase 3 quirks list.

## Suggested Fix
Update the README schema block to the live 12-key set. One-line edit pass.

## Note
No `audit-infra` label in repo — mapped to `tech-debt` + `documentation` (doc-rot).
