# REN-D17-07: Stale spec-color-as-F0 comment block in triangle.frag contradicts live F0 assignment

Labels: low, renderer, documentation

## Description

Still documents, in the present tense, the spec-colour-as-F0 branch that `31c99bb3` deleted ("So for PBR materials we use the authored spec_color as F0 directly"), reversing course only in its final third. There is no such branch: `F0` is assigned exactly twice, both `f0Dielectric`-derived. The stale half also contradicts the live CPU contract described by #2703, so a reader trusting it looks for the bug in the wrong layer — on the single largest comment block in the F0 region, which is where someone goes to ask "why does my FO4 metal panel look plastic".

## Location

`crates/renderer/shaders/triangle.frag` (the ~60-line F0 comment block)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D17-07).

https://github.com/matiaszanolli/ByroRedux/issues/2808
