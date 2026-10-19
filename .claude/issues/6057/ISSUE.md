# FNV-2026-10-18-08: The new FNV gate `m47-3-obscript.sh` is absent from the contract checker; `p4-quest-route.sh` is committed non-executable

- **Issue**: #6057 (https://github.com/matiaszanolli/ByroRedux/issues/6057)
- **Labels**: low, test-gap, tech-debt, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D6-02. **Status**: NEW.
- **Evidence**: `scripts/check-playable-smoke-contracts.sh` (~line 160, the #5427 list = dt1 / m42 / sm1 / dt2) does not name `m47-3-obscript.sh`. The gate does exit 77 with a SKIP line when data is missing (verified directly), so the contract holds today but is not enforced. The skill rule is "a new FNV gate absent from the checker is a finding". `docs/smoke-tests/p4-quest-route.sh` (Skyrim-only) is mode `100644`, so direct execution exits 126.
- **Suggested Fix**: add `m47-3-obscript.sh` to the checker loop; `chmod +x` and add `p4-quest-route.sh` to the checker or the workflow.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

