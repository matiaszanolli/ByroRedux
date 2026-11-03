# RT-2026-10-31-04: `m47-2-bleedout` red: an essential actor recovers at 57 health, the gate pins 30

Labels: medium,scripting,test-gap,bug,game:skyrim
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-04 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `m47-2-bleedout.sh:114` still compares against the literal `30`.

- **Severity**: MEDIUM (declared gate red; either the gate or the Skyrim NPC health composition is stale)
- **Dimension**: Gate matrix
- **Location**: `docs/smoke-tests/m47-2-bleedout.sh:114`; `crates/scripting/src/bleedout.rs:244-285` (`target = health_regain * undamaged`)
- **Status**: NEW
- **Game / Cell**: skyrim_se / RiverwoodSleepingGiantInn (Delphine)
- **Baseline**: PASS when the gate landed (`dd843f9e6`)
- **Current**: "Delphine recovered at 57 health, not 30" (every other leg passes: she enters bleedout, does not die, gets back up).
- **Description**: The engine recovers at `fEssentialHealthPercentReGain` (0.3) of the undamaged Health. 57 = 0.3 × 190, so Delphine's composed Health is now 190 where the gate assumes 100. The Skyrim pool now carries a class/level term (#5914/#5915, documented by `27f022dfd`). The gate was never re-derived. Which side is right (190 is plausible for the composed value) is not established here.
- **Evidence**: `/tmp/audit/runtime/gates/m47-2-bleedout.log`, `h-m47-2-bleedout.log` (same under Xvfb).
- **Suggested Fix**: Read the expected value from the running engine (`GetActorValue` undamaged × the GMST) instead of the literal 30.

## Completeness Checks
- [ ] **SIBLING**: The same harness defect checked in the other gates of its family
- [ ] **TESTS**: The gate is re-run live and passes (or SKIPs with 77 on missing data)
