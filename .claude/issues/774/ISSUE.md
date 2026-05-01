# Issue #774: FO3-1-PARGATE — BSShaderPPLightingProperty parallax-scalar gate is `>= 24`, nif.xml says `> 24`

**Severity**: MEDIUM · **Domain**: nif-parser, legacy-compat · **Type**: bug
**Source audit**: docs/audits/AUDIT_FO3_2026-05-01.md
**Game affected**: any FO3 mesh shipping at BSVER=24 (boundary case)
**Bundles**: FO3-1-01 (the off-by-one) + FO3-1-02 (cosmetic comment hygiene that perpetuates the inversion pattern)

## Summary

`crates/nif/src/blocks/shader.rs:96` reads parallax_max_passes + parallax_scale when `bsver >= 24`. nif.xml line 6247-6248 specifies `vercond="#BSVER# #GT# 24"` (strictly greater). The correct gate is `bsver > 24` (== `bsver >= 25`). FO3 ships content at BSVER=24 which over-reads 8 phantom bytes; `block_sizes` re-aligns the outer dispatch loop so the defect is masked at the recoverable-rate metric.

Sibling: line 89 has `bsver >= 15` (mathematically equivalent to `> 14`, but the inverted phrasing is what drove the parallax off-by-one). Flip both to `> N` form so the spec phrasing sits next to the code.
