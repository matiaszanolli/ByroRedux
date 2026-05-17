# #1126 — REN-D6-NEW-01: BLOOM_INTENSITY / VOLUME_FAR duplicated as const float AND #define in composite.frag — latent build break

**Severity**: LOW
**Domain**: renderer
**Status**: CLOSED (fixed in this session)

See GitHub issue for full body; brief: composite.frag redeclared `BLOOM_INTENSITY` and `VOLUME_FAR` as `const float` after `#include`-ing them as `#define`s. After preprocessor expansion the const-lines became syntactically invalid GLSL (`const float 0.15 = 0.15;`), breaking recompile-from-source. Fixed by dropping the two const declarations; moved their rich doc-blocks to `shader_constants_data.rs` alongside the canonical Rust consts. Drift tests polarity-flipped to assert ABSENCE of the local consts.
