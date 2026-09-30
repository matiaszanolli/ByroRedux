# #4872: REN-D5-2026-09-24-03: memory-budget.md ledger drift — model-tier instance tail (0/14/28 MiB, not 7) and dangling volumetrics noise-volume row

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D5-2026-09-24-03**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: Memory/Lifecycle; MERGED with D5-06

`memory-budget.md`'s model-tier instance-tail figure (7 MiB / 14 MiB across FIF) is never the realised growth: `grown_instance_capacity` doubles (65,536 → 131,072 → 262,144), so the real delta is 0, 14 or 28 MiB per slot (56 MiB across FIF at the top step), i.e. up to ~42 MiB under-ledgered, and the tier can force the jump to `MAX_INSTANCES` on dense exteriors. The "SKYAL cloud noise" row promises the volumetrics pass's private density-noise volumes (`volumetrics_base_noise` 64³ + `volumetrics_detail_noise` 32³, 294,912 B) are counted in the Volumetrics section; they are not.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

