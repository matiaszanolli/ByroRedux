# #4014 — REN-2026-09-06-D18-02: `sample_dalc_cube`'s TOD-fold justification cites a line range that holds unrelated content and a parser rule that does not exist

**Labels**: low, renderer, terrain-exterior, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D18-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` (`sample_dalc_cube`); cited target `crates/plugin/src/esm/records/weather.rs`
- **Status**: NEW
- **Description**: `sample_dalc_cube`'s doc explains why it folds the six sky
  TOD slots onto DALC's four: *"fold high_noon→day and midnight→night per the
  WTHR parser's on-disk padding rule (`crates/plugin/src/esm/records/weather.rs:312-314`)"*.
  Both halves of that citation fail:

  1. **The line range is stale.** `weather.rs:310-320` today is the
     `cloud_textures` / `skyrim_cloud_textures` field documentation
     (*"Cloud texture paths. FNV/FO3 ship 4 layers (DNAM/CNAM/ANAM/BNAM …)"*) —
     nothing about DALC or TOD padding.
  2. **The named rule does not exist.** The parser has no high_noon→day /
     midnight→night mapping anywhere. The only DALC slot logic is the
     truncated-record backfill at the end of `parse_weather` (*"Fill missing
     slots with the most recent one … defensive against truncated mod
     records"*), which is about absent sub-records, not about mapping six
     slots onto four. The `SkyrimAmbientCube` doc and the
     `skyrim_dalc_per_tod` field doc both just state "4 entries: sunrise / day /
     sunset / night".

  The fold is a *consumer-side* decision made in `sample_dalc_cube` itself and is
  perfectly defensible — it just has no upstream authority, and the comment
  claims one.
- **Evidence**: `sed -n '305,320p' crates/plugin/src/esm/records/weather.rs`
  returns cloud-texture field docs; `grep -n "DALC" crates/plugin/src/esm/records/weather.rs`
  finds no TOD-fold logic, only `SKYRIM_DALC_SIZE`, `parse_skyrim_dalc`, and the
  backfill loop.
- **Impact**: Doc-rot in a load-bearing spot — a reader checking whether the
  fold is correct is sent to the wrong file region and told a non-existent
  parser rule sanctions it. This is precisely the line-anchor rot the audit
  discipline warns about, sitting in production code rather than an audit report.
- **Related**: #993 (DALC TOD interpolation), #2816 (the cross-fade fix that
  extracted this helper), `flat_dalc_cube`.
- **Suggested Fix**: Point the comment at `SkyrimAmbientCube`'s own doc (the
  "4 entries: sunrise / day / sunset / night" statement) by symbol, not line
  number, and reword the justification as a consumer-side mapping decision
  rather than a parser rule.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
