# #4336 — TD1-001: Dimension 1's `prod_loc` helper counts braces inside string literals, so its production-LOC numbers are wrong in both directions

**Labels**: medium, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4336

- **Severity**: MEDIUM (stale audit baseline that misled an audit — this one) · **Dimension**: 1 · **Kind**: tech-debt (audit infrastructure) · **Effort**: small
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:92-116` (the awk body, Phase 1 step 5)
- **Status**: NEW · **Age**: helper added under #3081; first false positive 2026-09-14
- **Finding**: Inside a `#[cfg(test)]` block the awk tracks depth by counting every `{`/`}` on the line without stripping string/char literals or comments. Test format strings (`"{call}"`) and source-scan splits (`split("\n}\n")`) close the block early or never. Measured against a string-aware counter: `crates/scripting/src/translate/effects.rs` 2093 vs **1681** (+412; its `#[cfg(test)]` is at `:1682`, confirmed); `crates/renderer/src/vulkan/groundcover.rs` 1966 vs 1680 (+286); `crates/nif/src/blocks/shader.rs` 2071 vs 2058; `crates/renderer/src/vulkan/context/draw.rs` 1890 vs **1957** (−67). It also only matches `#[cfg(test)]` at column 0.
- **Impact**: It produced a false "NEW crossing" that would have filed a split for a file 319 lines under threshold, and it under-reports the repo's most-regrown file.
- **Suggested Fix**: Strip `//`, `/* */`, `"…"`, `r#"…"#` and char literals before counting. Better: move the helper to a checked-in script under `tools/` with fixtures for the effects.rs and draw.rs shapes, then re-derive the Phase-1 orientation figures (see TD4-001).

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
