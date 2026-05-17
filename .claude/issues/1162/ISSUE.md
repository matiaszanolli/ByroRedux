# #1162 — REN-D10-NEW-10b: triangle.frag DBG_* const-redeclarations shadow shader_constants.glsl #defines

**Severity**: LOW
**Domain**: renderer
**Status**: OPEN
**Source**: Surfaced during the implementation of #1157.

## Location

`crates/renderer/shaders/triangle.frag:755-841` (10 `const uint DBG_*` declarations that shadow the matching `#define` macros from `include/shader_constants.glsl:54-63`).

## Description

After #1119 / TD4-204 consolidated the `DBG_*` bit catalog into `shader_constants.glsl`, every `DBG_*` bit is `#define`d there. But `triangle.frag` retains the original `const uint DBG_BYPASS_POM = 0x1u;` etc. declarations at lines 755-841. After `#include "include/shader_constants.glsl"` at line 8 expands the `#define`s into scope, each of these 10 declarations textually substitutes to invalid GLSL.

Same shape as #1126 (composite.frag BLOOM_INTENSITY / VOLUME_FAR) and #1151 (cluster_cull.comp THREADS_PER_CLUSTER). All three are blocking recompile-from-source for their respective shaders after #1157 lands.

## Evidence

```
$ cd crates/renderer/shaders && glslangValidator -V triangle.frag -o /tmp/test.spv
ERROR: triangle.frag:755: '' :  syntax error, unexpected UINTCONSTANT, expecting COMMA or SEMICOLON
```

## Suggested Fix

Drop the 10 `const uint DBG_*` declarations at `triangle.frag:755-841` (and move the doc-blocks to `src/shader_constants_data.rs` alongside the canonical Rust mirror). Polarity-flip the `triangle_frag_dbg_bits_match` drift test.

## Related

- #1157 — REN-D10-NEW-10 / 7 shaders missing `GL_GOOGLE_include_directive` (upstream; recompile possible only after BOTH land)
- #1126 — composite.frag BLOOM_INTENSITY + VOLUME_FAR (same pattern)
- #1151 — TD4-302 / cluster_cull.comp THREADS_PER_CLUSTER (same pattern)
- #1119 — TD4-204 / DBG_* consolidation (the change that introduced the shadowing)
