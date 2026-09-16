# #4440: SF-2026-09-16-D7-01: Starfield NIF parse is 100.00% clean, but ROADMAP, the compatibility docs, this skill and the 09-11 audit still report 99.98% with a 19-file residual tail

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4440
- **Labels**: low,documentation,doc-rot,game:starfield,legacy-compat
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW
- **Dimension**: 7
- **Location**:
  - `ROADMAP.md:519, 686, 1528`
  - `docs/engine/game-compatibility.md:19, 44, 201`
  - `docs/engine/nif-parser.md:676`, which also gives a third total, "120 836",
    and says "residual truncation tail tracked at #2105/#3524"
  - `.claude/commands/audit-starfield/SKILL.md` Dimension 7
  - `docs/audits/AUDIT_STARFIELD_2026-09-11.md:167-176`
- **Status**: NEW
- **Description**: The 6 MeshesPatch and 13 ShatteredSpace-Main01 truncations
  no longer reproduce. #3524 (the characterised `BSWeakReferenceNode`
  residual) closed on 2026-09-08. The 09-11 audit wrote "confirmed unchanged"
  but live-measured only the two LODMeshes archives, which were already at
  100%.
- **Evidence**: The gate output quoted above.
- **Impact**: Stale status only. The skill still tells auditors to confirm a
  19-file tail "has not grown", which is now unfalsifiable busywork.
- **Suggested Fix**: `/session-close` should refresh the matrix rows to
  100.00% (120,543 / 120,543) and retire the residual-tail language. Update
  the skill's Dimension 7 to "confirm it stays at 0".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
