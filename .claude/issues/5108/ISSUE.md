# TD3-2026-09-29-03: docs/feature-matrix.md contradicts the player body and dialogue features shipped 09-28/09-29

**Labels**: low,gameplay,dialogue,tech-debt,documentation,doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**:
  - `docs/feature-matrix.md:246`: "container/corpse transfer, visible player-mesh attachment, general HUD
    bars, and quest-objective presentation remain open".
  - `docs/feature-matrix.md:207`: "Dialogue tree + dialogue UI integration | ✗ M43 remainder".
- **Evidence**:
  - The player mesh shipped in `a070baaad` (body + view toggle), `db8351587` (third-person walk/idle) and
    `0182fc5e8` (mid-life gear import). `ROADMAP.md:239` lists it with `p3-player-body.sh`.
  - Dialogue shipped in `ab31cfefe` / `766e1746e` (NPC activation → topic selection, native response
    surface). `ROADMAP.md:240` says "live-verified in MarkarthWarrens".
  - The matrix was last touched 09-27.
  - The container-transfer clause should also be re-checked against `container_loot_system` (#4712).
- **Impact**: `_audit-common.md` names the matrix as the status floor audits re-check. These two rows
  would lead a gameplay or UI audit to scope shipped features as absent.
- **Related**: sibling matrix findings AUD-2026-09-29-D5-04 and CHAR-2026-09-29-D5-01 cover other rows.
- **Suggested Fix**:
  - Drop "visible player-mesh attachment" from row 246.
  - Mark row 207 as "~ single-level topic selection + native response surface (P4); tree/UI open".

**Validated at HEAD 9fcfdc3fc**: `docs/feature-matrix.md:207` still reads `Dialogue tree + dialogue UI integration | ✗ M43 remainder`; `:246` still lists "visible player-mesh attachment" as open.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
