# EX-10/11 item 8: active VWD full-model culling (decouple full-REFR spawn radius from radius_unload)

## Description

Split out of #2371 (EX-10/11 item 8) per that issue's own stated intent ("scope as its own follow-up issue rather than folding into EX-10/11 closure") — filed now since it was never actually spun off.

## Background

The VWD (VisibleWhenDistant) flag is fully parsed (#1731) and materialized per placement as the `VisibleWhenDistant` marker (#1889), with **no render-time consumer today, by design**. Full REFRs only ever spawn inside `radius_unload`; both terrain and object LOD rings load strictly outside it — so a full model and its `.bto`/LOD proxy structurally never coexist under the current streaming radii, and there's nothing to cull yet.

The **detection half** already landed (commit `2a84ab97`): `LodCoverageStats::vwd_full_model_overlaps` audits, live, that a resident `VisibleWhenDistant`-flagged REFR's cell never falls inside a resident object-LOD quad's footprint — reads 0 on every real session today, proving the ring-separation argument holds, and becomes the regression gate for whoever builds the active cull.

## What's actually needed

Building the *active* VWD cull requires giving VWD-flagged REFRs their own streaming radius **beyond** `radius_unload` — today's streaming is whole-cell granularity only; nothing spawns an individual REFR independent of its cell. `docs/engine/exal.md` §5.2 already states this decoupling "needs real-game visual validation before it is enabled" — building it and shipping disabled would be real effort with no way to prove correctness beyond synthetic tests; enabling it blind is exactly the kind of visually-consequential, `cargo-test`-invisible change this project's standing policy says not to ship speculatively (`feedback_speculative_vulkan_fixes`, generalized to any render-visible streaming-radius change).

## Suggested approach

1. Design the per-REFR VWD streaming radius as an addition to (not a replacement of) `radius_unload` — needs to reintroduce the #1866 overlap risk carefully, since that's exactly what the current ring-separation argument prevents by construction.
2. Wire the actual cull consumer (skip drawing/spawning the full REFR once it enters `radius_unload` proper, relying on the LOD proxy instead — or the inverse, depending on the chosen direction).
3. Validate live against a location with real VWD-flagged content and confirm no visual pop/seam at the transition boundary.
4. `LodCoverageStats::vwd_full_model_overlaps` already gates the regression case (a VWD REFR and its LOD quad proxy must never both render) — keep it green throughout.

## Related
#2371 (EX-10/11, parent). #1866/#1889/#1731 (VWD parse/marker history). `docs/engine/exal.md` §5.2.

