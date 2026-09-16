# #4446: FO4-D9-2026-09-16-04 / SF-2026-09-16-META-01: audit skills still reference the split `crates/nif/src/blocks/shader.rs` (10 refs, 6 skills) — `_audit-validate.sh` is red

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4446
- **Labels**: low,tech-debt,documentation,doc-rot
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_FO4_2026-09-16.md + docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

Reported independently by two audits on 2026-09-16; merged into one issue.

## From `docs/audits/AUDIT_FO4_2026-09-16.md` — The `/audit-fo4` skill points Dimension 5 at `crates/nif/src/blocks/shader.rs`, which `eaa94b49d` split into `blocks/shader/`, so the validate gate now fails; it also backticks the deleted `select_finest_lod`

- **Severity**: LOW (audit-infrastructure doc rot)
- **Dimension**: 9 (skill/forward-scope hygiene)
- **Location**: `.claude/commands/audit-fo4/SKILL.md:121` (Dimension 5 entry points) and `:76` (the `select_finest_lod` mention)
- **Status**: NEW. `eaa94b49d` ("split the ESM records barrel and the NIF shader family file", from #4339) turned `blocks/shader.rs` into `crates/nif/src/blocks/shader/` (`mod`, `lighting`, `effect`, `legacy`, `sky_water`).
- **Description**: `.claude/commands/_audit-validate.sh` currently fails with 10 stale path references, all of them `crates/nif/src/blocks/shader.rs`. One is in this skill; the others are in audit-fo3, audit-nif, audit-skyrim, audit-starfield and audit-tech-debt. The symbol advisory also lists `select_finest_lod` for audit-fo4. The skill prose itself says that function was removed in `66fff31b`, so by the path-reference convention it should be italicised, not backticked.
- **Evidence**: `bash .claude/commands/_audit-validate.sh` prints `STALE: .claude/commands/audit-fo4/SKILL.md:121 — crates/nif/src/blocks/shader.rs` and ends with `FAIL: 10 stale path reference(s)`. `ls crates/nif/src/blocks/` shows `shader/` and `shader_tests/`.
- **Impact**: The validate gate is red for every audit-skill edit until this is fixed. A Dimension 5 auditor following the path finds nothing; `BSLightingShaderProperty` now lives in `crates/nif/src/blocks/shader/lighting.rs`.
- **Related**: #4339, #4362, #1114 (path-reference convention)
- **Suggested Fix**: Re-point the six skills at `crates/nif/src/blocks/shader/lighting.rs` (and `shader/effect.rs` where the text means `BSEffectShaderProperty`). Italicise *select_finest_lod* in audit-fo4. Re-run the validate gate.

---

## From `docs/audits/AUDIT_STARFIELD_2026-09-16.md` — The audit path gate fails — `crates/nif/src/blocks/shader.rs` is backticked 10 times across 6 skills (including this one) after the `shader/` split

- **Severity**: LOW
- **Dimension**: Audit infrastructure (tech-debt)
- **Location**:
  - `.claude/commands/audit-starfield/SKILL.md:43, 315`
  - `.claude/commands/audit-fo3/SKILL.md:92`
  - `.claude/commands/audit-fo4/SKILL.md:121`
  - `.claude/commands/audit-nif/SKILL.md:65, 115`
  - `.claude/commands/audit-skyrim/SKILL.md:77, 87, 126`
  - `.claude/commands/audit-tech-debt/SKILL.md:129`
- **Status**: NEW (`eaa94b49d`, 2026-09-15; #4339 recorded the split but not
  the skill fallout)
- **Description**: `SKIP_SYMBOL_CHECK=1 .claude/commands/_audit-validate.sh`
  ends with `FAIL: 10 stale path reference(s)`. Every hit is the pre-split
  file, which is now `crates/nif/src/blocks/shader/{mod,legacy,sky_water,lighting,effect}.rs`.
- **Impact**: The gate is red for every audit skill edit. The per-game skills'
  Dimension 6 entry points send auditors to a missing file (the family now
  lives in `shader/lighting.rs` / `shader/effect.rs`).
- **Suggested Fix**: Repoint each reference at the family file that holds the
  cited symbol: `parse_skyrim_shader_base` → `shader/mod.rs`,
  `BSLightingShaderProperty` → `shader/lighting.rs`,
  `BSEffectShaderProperty` → `shader/effect.rs`.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
