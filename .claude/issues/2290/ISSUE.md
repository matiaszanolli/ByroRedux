# SCR-D5-NEW5-03: translate/source.rs's module doc still claims 'no .pex parser exists'

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2290
**Source audit**: `docs/audits/AUDIT_SCRIPTING_2026-08-03.md`
**Severity**: LOW (doc rot only)
**Dimension**: Recognizer-Chain Soundness (Dimension 5)
**Location**: `crates/scripting/src/translate/source.rs:17-20`
**Labels**: low, tech-debt, documentation

## Body

(see GitHub issue for full body — description, evidence, impact, suggested fix)

Summary: `source.rs`'s doc comment says a `.pex` frontend "is intentionally NOT a variant yet because no `.pex` parser exists," but `translate_pex` (`crates/scripting/src/translate/mod.rs:94`) has parsed and decompiled `.pex` bytes since commit `c5293ef7` (2026-06-22). Cosmetic-only fix: update the paragraph to describe the existing `translate_pex` path.
