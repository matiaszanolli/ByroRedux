# REN-D17-08: distributionGGXAniso/deriveAxAy anisotropic contracts have no automated regression guard

Labels: low, renderer, tech-debt, bug

## Description

The #1250 isotropic-degeneracy contract and the #1254 anisotropic clamp have **zero** automated guards, unlike every sibling invariant in this dimension (#2243, #2244, #2472, #1190 all have string-mirror tests with negative assertions). `grep -rn "distributionGGXAniso\|deriveAxAy" --include=*.rs` → nothing. Both contracts were verified **algebraically** to hold today; the exposure is purely regression, in a lobe with no CPU producer, so a break would not be caught by eyeball either.

## Location

`crates/renderer/shaders/include/pbr.glsl` (`distributionGGXAniso`, `deriveAxAy`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D17-08).

https://github.com/matiaszanolli/ByroRedux/issues/2810
