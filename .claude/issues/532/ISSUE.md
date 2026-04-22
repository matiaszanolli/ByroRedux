# FNV-ANIM-4: KFM parser not wired to actor-controller loading

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/532
- **Severity**: LOW (milestone-gated on M30)
- **Dimension**: Animation
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/nif/src/kfm.rs` (full parser, no external callers)
- `byroredux/src/scene.rs:315-369` (single `--kf` path)

## Summary

KFM binary parser complete + tested for versions 1.2.0.0–2.2.0.0. No call site. FNV BSAs ship `character.kfm` / `creature.kfm` with sequence-transition graphs, but there's no "load this actor's KFM" path. Blocks actor-controller scaffolding.

Fix: add `KfmCatalog` resource with `find_sequence(&str) → clip_handle`. Bridges KFM to `AnimationStack.play(...)`.

Fix with: `/fix-issue 532`
