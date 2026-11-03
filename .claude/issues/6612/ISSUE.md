# RT-2026-10-31-05: `m-exteriors cycle` red on fnv, fo3 and skyrim: the `composite_term` sun-cycle pixel gate cannot see noon vs night

Labels: medium,terrain-exterior,test-gap,bug,game:fnv,game:fo3,game:skyrim
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-05 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `m-exteriors.sh` is unchanged since the report; not re-run at publish. Related closed #4491 (added the pixel invariant).

- **Severity**: MEDIUM (declared gate red on three of five games)
- **Dimension**: Gate matrix / exterior rendering
- **Location**: `docs/smoke-tests/m-exteriors.sh:657-669` (the #4491 pixel invariant), fixtures' cycle poses
- **Status**: NEW (the pixel invariant is the #4491 addition; the 10-27 report did not run `cycle`)
- **Baseline**: n/a
- **Current**: fnv noon 0.2770 / night 0.2523 / sd 0.0388 (delta 0.0247 passes, but sd < 0.039); fo3 noon 0.2557 / night 0.2529 / sd 0.0293; skyrim noon 0.2643 / night 0.2530 / sd 0.0280. Oblivion and FO4 PASS. Identical under Xvfb.
- **Description**: The `fo3` `noon.png` frame is a close-up of a near-black wall with a door prompt; `night.png` is black. The pose faces a wall, so `composite_term`'s mean cannot reflect the sky or the sun. Either the fixture pose is wrong for this invariant, or the sun/sky is not reaching `composite_term` on these games. The CPU-side sun values (`sun: intensity=…`) do change; the day phase is selectable (`m34-day-night` passes its first six legs).
- **Evidence**: `/tmp/byro-exterior-smoke.61gqnD/fo3/{noon,night}.png` and `*-composite-term.png`; `summary.tsv`.
- **Related**: AUDIT_EXTERIOR_2026-09-19 EXT-D7-04 (the gate that was meant to add this check).
- **Suggested Fix**: Look at the three `noon-composite-term.png` captures. If they show a wall, re-aim the cycle pose upward; if they show sky and the term is flat, route it to `/audit-exterior` as a sky-assembly defect.

## Completeness Checks
- [ ] **SIBLING**: The same harness defect checked in the other gates of its family
- [ ] **TESTS**: The gate is re-run live and passes (or SKIPs with 77 on missing data)
