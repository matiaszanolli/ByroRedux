# FO4-2026-10-31-02: #6471 leaves FO4's graph-less looping props frozen

Labels: low,bug,animation,legacy-compat,game:fo4

Filed from `docs/audits/AUDIT_FO4_2026-10-31.md` (finding `FO4-2026-10-31-02`).

- **Severity**: LOW. Visual only, about 150 REFRs, and the engine's own selection rule for these is unsourced.
- **Dimension**: 4, FO4 spawn through the shared sequence mechanism.
- **Location**: `crates/core/src/animation/named_sequences.rs:28-38` (`spawn_handle`) and `byroredux/src/cell_loader/spawn.rs:937-960`.
- **Status**: NEW. This is the FO4 leg of FNV-2026-10-31-02, which is the same #6471 residual on FNV data.
- **Description**:
  - `spawn_handle` plays `Idle`, else `SpecialIdle` (an exact name match), else nothing.
  - FO4 does not use those names for its loops: `windmin`/`windmax` for cloth, `simpleidle` for beams, `specialidle_areaeffect` (a different name from `specialidle`) for effect meshes, `autoplay`/`autoloop`.
  - Where the NIF has a graph the behavior route owns the animation (see -01). Where it has none, the first-listed looping sequence used to play and no longer does.
- **Evidence** (probes `fo4probe`, `refs`):
  - Of 1,626 FO4 meshes with manager sequences, 124 have `Idle`/`SpecialIdle`, 1,502 have neither, and 83 of those have a looping first sequence and no graph.
  - Fallout4.esm placements of such bases (graph-less, no idle, first sequence a loop): 21 MSTT bases (119 REFRs), 12 STAT bases (28 REFRs) and 2 ACTI bases (2 REFRs). Examples: `BannerBaseball*`, `DiamondRedTarp*`, `DiamondPowerCloth*`, `DiamondShackCloth*`, `CG_Nuke`, and the vault-entrance valve (`stage1`/c0).
- **Impact**: Diamond City cloth and banners sit in the bind pose.
- **Related**: #6471, FNV-2026-10-31-02, -01 above.
- **Suggested Fix**: Decide the FO4 rule from a source: the CK or an engine reading of which sequence the game starts for a graph-less placed object. Do not extend the name list by analogy. If the rule is "start the first looping sequence", encode it in `spawn_handle` behind the same contract and extend its test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
