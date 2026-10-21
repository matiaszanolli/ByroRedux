# #6275: TD3-2026-10-21-01: status floors still say FO4 ragdolls are blocked, after #6070 shipped them live

**Labels**: low,physics,game:fo4,tech-debt,documentation,doc-rot

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: LOW · **Dimension**: 3 · **Location**: `docs/feature-matrix.md:178` ("FO4+ blocked on BhkSystemBinary"), `ROADMAP.md:103` ("FO4+ ragdolls wait on the `BhkSystemBinary` decoder")

- **Severity**: LOW (no audit shown misled yet; the feature matrix is the status floor audits read, so it becomes MEDIUM if one is)
- **Dimension**: 3
- **Location**: `docs/feature-matrix.md:178` ("FO4+ blocked on BhkSystemBinary"), `ROADMAP.md:103` ("FO4+ ragdolls wait on the `BhkSystemBinary` decoder")
- **Status**: NEW for the two status docs. They are siblings of PHYS-D3-2026-10-21-04 (code comments: `combat.rs:1055`, and per the orchestrator `asset_provider/actor_graph.rs:161`) and NIF-D5-2026-10-21-03 (`physal.md` item 6). A third stale code comment, `byroredux/src/systems/graph_locomotion.rs:446` ("FO4 ragdolls do not activate yet to override the pose"), belongs with PHYS-D3-04 and is not filed here.
- **Age**: `7c6e6c3e1` (2026-10-20, #6070 "live FO4 ragdoll smoke; keep the decoded articulation and retire the stand-in capsule"), `019b087e3` (#6066), `fc4a15104` (#6067); CLAUDE.md already lists `docs/smoke-tests/fo4-ragdoll.sh`
- **Effort**: trivial
- **Description / Evidence**: FO4 skeleton ragdolls are decoded (`decode_np_ragdoll`, #5015), articulated and activated on death, and gated by a live smoke (two feral ghouls and a human corpse posed on 18-body ragdolls, kills activate finite ragdolls). The matrix row and the ROADMAP gap list still say they are blocked.
- **Impact**: Per-game audits (fo4, physics) and the next `/session-close` read the matrix as the floor.
- **Related**: PHYS-D3-2026-10-21-04, NIF-D5-2026-10-21-03, #6161 (other matrix rows).
- **Suggested Fix**: Flip `:178` to "✓ Oblivion → Skyrim classic chain; FO4 decoded `bhkRagdollSystem` (#6070); FO76/SF tagfile payloads open (NIF-D5-01)". Drop the clause from `ROADMAP.md:103`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
