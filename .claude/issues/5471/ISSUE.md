# #5471: TD3-2026-10-08-02: ROADMAP / feature-matrix / npc-spawn-ai-packages.md still say Eat, Sleep and Dialogue procedures have no runtime; Dialogue (force-greet) shipped 10-07, Eat/Sleep at HEAD

**Labels**: low,ai,tech-debt,documentation,doc-rot,game:fnv
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5471

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD3-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `ROADMAP.md:320` (the M42 row: "Seven procedures …"; "Open: the 10 non-locomotion procedures
    (Find/Eat/Sleep/…/Dialogue/UseWeapon)")
  - `docs/feature-matrix.md:101-106` ("The remaining 10 procedures (Find/Eat/Sleep/…/Dialogue/UseWeapon) are parse-only")
  - `docs/engine/npc-spawn-ai-packages.md:172-180` (only seven procedures "have a name and a consumer") and `:518-526`
    ("Seven procedures of ~17 execute … No Find/Eat/Sleep/…/Dialogue … runtime exists anywhere")
  - `ROADMAP.md:79` says "Story Manager (#5366 Phases 0–1, 2026-10-07)", while `:120` says "Phases 0–4 shipped" and #5366
    is CLOSED.
  - The new smoke script `docs/smoke-tests/m42-eat-sleep.sh` is in neither `docs/smoke-tests/README.md` nor the CLAUDE.md
    smoke list. Its siblings `dt1`/`dt2`/`sm1` are in both.
- **Status**: NEW
- **Age**:
  - Dialogue procedure: `14cff35ae` (#5367 Phase F, 2026-10-07). `byroredux/src/systems/forcegreet.rs` is "the Dialogue AI
    package procedure (FO3/FNV `PKDT` procedure 15)". `5a1eecf6b` updated the ROADMAP's dialogue text the same day but not
    the M42 row.
  - Eat/Sleep: `00f580e09` (HEAD). `PROCEDURE_EAT`/`PROCEDURE_SLEEP` are at `crates/plugin/src/esm/records/misc/pack.rs:323,328`,
    dispatched from `byroredux/src/npc_spawn/ai_package.rs` and run by `eat_sleep_system` (Stage PostUpdate).
- **Effort**: trivial
- **Impact**:
  - `feature-matrix.md` is the status floor, and `npc-spawn-ai-packages.md` is the reference the gameplay and scripting audits
    are told to believe.
  - Both now under-state shipped procedures, so an auditor would treat `eat_sleep_system` / `forcegreet_system` as
    unexpected code.
  - The M42 Dialogue entry also contradicts M43's own dialogue text.
- **Related**: #5367 (OPEN, the dialogue-trees tracker).
- **Suggested Fix**:
  - Say "10 procedures" (adding Eat/Sleep v0 and Dialogue/force-greet) in the three docs, and drop Eat/Sleep/Dialogue from
    the "open" lists.
  - Update ROADMAP:79 to Phases 0–4.
  - Index `m42-eat-sleep.sh` in the smoke README and CLAUDE.md.

## Also reported as `FNV-2026-10-08-D5-03` (AUDIT_FNV_2026-10-08.md)

Cross-report duplicate merged at publish time; the sibling report's text follows.

**Source**: `docs/audits/AUDIT_FNV_2026-10-08.md` — `FNV-2026-10-08-D5-03` (HEAD `00f580e09`)

- **Severity**: LOW (doc rot).
- **Dimension**: Ambient AI.
- **Location**:
  - `ROADMAP.md:320` (M42 row: "**Open:** the 10 non-locomotion procedures (Find/Eat/Sleep/…/Dialogue/UseWeapon)"). This contradicts its own line 100 and the M43 row, which ship force-greet.
  - `docs/feature-matrix.md:89-105` ("7 of ~17 `PACK` procedures") and `:339`.
  - `docs/engine/npc-spawn-ai-packages.md:177-178` and `:522-525` ("No Find/Eat/Sleep/…/Dialogue … runtime exists anywhere").
- **Status**: NEW. `00f580e09` touched both docs only to bump the CTDA count 21 → 22. No sibling report flags it.
- **Description**: `from_package` now dispatches `PROCEDURE_EAT` (3), `PROCEDURE_SLEEP` (4) and `PROCEDURE_DIALOGUE` (15) (`byroredux/src/npc_spawn/ai_package.rs:197-219`) into `eat_sleep_system` and `forcegreet_system`. These procedure ids exist only in the FO3/FNV `PKDT` enum. The trace doc and both status docs still describe them as missing subsystems.
- **Suggested Fix**: move Eat/Sleep/Dialogue to "Shipped" with their v0 caveats (sit-marker fallback per GAME-D5-02, PLDT centre per GAME-D5-03, the fail-open install per GAME-D5-01). Update the procedure count and the trace doc's §§ on unbuilt procedures.

## Completeness Checks
- [ ] **SIBLING**: ROADMAP M42 row + :79 Story Manager phases, `docs/feature-matrix.md`, `docs/engine/npc-spawn-ai-packages.md` (both sections), smoke README and CLAUDE.md smoke list all updated together
