# FO3-2026-09-11-D2-03: _audit-common.md points extract_emitter_params/extract_emitter_rate at walk/mod.rs; both live in walk/emitter.rs

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4244
**Labels**: documentation, low, legacy-compat, tech-debt, game:fo3, doc-rot
**Source**: `/audit-fo3` — `docs/audits/AUDIT_FO3_2026-09-11.md`, finding FO3-2026-09-11-D2-03

**Severity**: LOW
**Dimension**: NIF v20.2.0.7 Parser (FO3 Block Subset) — `/audit-fo3` Dimension 2
**Location**: `.claude/commands/_audit-common.md:16`

**Description**: `_audit-common.md`'s `NIF Import:` row attributes `extract_emitter_params`/`extract_emitter_rate` to `walk/mod.rs`; both functions actually live in `walk/emitter.rs`.

**Evidence**: `_audit-common.md:16` reads "walk/{mod, tests} (mod.rs carries extract_emitter_params/extract_emitter_rate)". Confirmed in current source: `extract_emitter_params` is defined at `crates/nif/src/import/walk/emitter.rs:245`, `extract_emitter_rate` at `crates/nif/src/import/walk/emitter.rs:375`. Neither symbol appears in `walk/mod.rs`.

**Impact**: Sent this session's auditor to the wrong file; contributed to FO3-2026-09-11-D2-01 (particle azimuth not forwarded, #4240) going unaudited on the forwarding half for months, since the shared audit reference pointed at the wrong module.

**Related**: #4240 (the finding this doc-rot contributed to going unaudited).

**Suggested Fix**: Update the `NIF Import:` row to list `walk/{mod, emitter, lights, node_attrs, texture_effect, tests}` and attribute `extract_emitter_params`/`extract_emitter_rate` to `emitter.rs`.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix; no regression test applicable.
