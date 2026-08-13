# REN-D15-08: watal.md §2 stale re #1502 fix status and resolve_water_material line cite

## Description
(a) Lists the #1502 procedural-noise banding as a *current* fragility; it is fixed — `sampleScrollingNormal` and `foamFlowStreaks` both subtract `originOffset` before hashing, and the textured branch stays absolute *deliberately* with the #2496 texel-integral bound. The Dim-15 brief asks that #1502 be recast as a regression guard; the doc contradicts that and invites a re-fix that would re-break the deliberate absolute-UV branch. (b) Cites `resolve_water_material` at `env_translate.rs:89-176`; the function is near line 352.

## Location
`docs/engine/watal.md` §2

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2790

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-08).
