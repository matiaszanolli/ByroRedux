# SPT-NEW-02: Stale crate-level module docstring claims "Phase 1.2 (recon scaffold) … ships only the version dispatch and the recon harness"

**Issue**: #1707
**Source audit**: `docs/audits/AUDIT_SPEEDTREE_2026-06-23.md`
**Severity**: LOW · **Labels**: low, tech-debt, documentation
**Dimension**: Tag Dictionary (doc-rot)
**Location**: `crates/spt/src/lib.rs:34-41`

## Description

The `crates/spt/src/lib.rs` module header (`## Status — Phase 1.2 (recon scaffold)`) reads "Today this crate ships only the version dispatch and the recon harness. The actual byte-level parser (Phase 1.3) lands once the recon results … partition ≥95 % of the FNV corpus." That is stale: the byte-level walker (`parser.rs`), tag dictionary (`tag.rs`), scene model (`scene.rs`), and placeholder importer (`import/mod.rs`) all shipped; the ≥95 % gate clears on all three games; the `.spt` REFR route + `--tree` visualiser are wired. The crate is at Phase 1.4/1.5, not 1.2.

## Evidence

`lib.rs` exports `parse_spt`, `import_spt_scene`, `dispatch_tag`, `SptScene` (`lib.rs:53-58`) — the "Phase 1.3" parser the docstring says has not landed yet. Confirmed in current tree.

## Impact

Doc-rot only. Understates what shipped. No runtime impact.

## Suggested Fix

Update `## Status` to "Phase 1.4/1.5 — parameter-section walker + placeholder-billboard fallback shipped; geometry-tail decode (Phase 2) deferred" and drop the "ships only version dispatch + recon harness" sentence.
