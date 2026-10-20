# TD3-2026-10-19-01: `docs/feature-matrix.md` (the "status floor") contradicts shipped code and itself in five rows

Labels: medium,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: MEDIUM (stale status doc that audit-oblivion / audit-gameplay / audit-scripting read as a floor; promoted per the stale-baseline row, demote to LOW if no audit is shown to have been misled)
- Dimension: 3 · Effort: small · Status: NEW (the 10-15 TD3-05 fixes landed; this is a different set of rows)
- Locations: `docs/feature-matrix.md:341` (heading "live gaps as of 2026-08-19"), `:350`, `:358`, `:238`, `:215-216`
- Description / Evidence:
  - `:350` "Oblivion exterior (TES4 worldspace + LAND)" is a gap, while `:21-26` and the paragraph at `:28-30` say it shipped (closed by #2368, 2026-08-12).
  - `:358` "UV scroll animated materials ... audited, not prioritised" vs `:130,134` (`AnimatedUvTransform` written by `animation_system`, consumed in `render/`, shipped).
  - `:238` "Authored attack/hit/death animation + sound | X | Open - combat sound tracked as #4747": `byroredux/src/systems/combat_anim.rs` (added `ec3a18d2f`, 2026-09-21, "Wire the P2 combat tail: Draugr attack/hit/death takes plus combat sounds") ships both; and #4747 is an unrelated open audio-docs issue (REGN music status). Wrong issue number + wrong status.
  - `:216` "Created-object spawning from aliases | X": `crates/scripting/src/created_references.rs` (`eeb6793b7` 2026-10-12 "Created Object 'At' aliases spawn their references", `3299c7dcb` PlaceAtMe) ships it. `:215` "Reference collections ... X": script-filled `RefCollectionAlias` shipped (`alias_collections.rs`, 2026-10-13); only condition/fill-type-filled collections are missing.
  - Heading date 2026-08-19 is two months old.
- Proposed fix: delete rows `:350` and `:358`; flip `:238` to partial/shipped for Draugr-family (cite `combat_anim.rs`, drop #4747); split `:215-216` into shipped (script-driven collections, created-object "At") vs open (condition-filled collections, LCTN); re-date the heading.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
