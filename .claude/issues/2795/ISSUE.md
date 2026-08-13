# REN-D7-2026-08-12-01: main.rs debug_assert_eq panics on MAX_MATERIALS over-cap where a degrade path is already implemented

- **Severity**: LOW
- **Dimension**: 7
- **Labels**: low,renderer,bug

## Description
Three authorities describe the `MAX_MATERIALS` over-cap path as a supported degrade (id 0 + warn-once); `main.rs` then `debug_assert_eq!`s the overflow count is zero, so a plain `cargo run` on a large/modded exterior **panics** where the degrade is already implemented, tested and documented. The same doc records the opposite call for `MAX_INSTANCES` (#956/#992). Reachable per the code's own recorded Skyrim radius-3 measurement (4000+ unique materials). Debug builds only.

## Location
`byroredux/src/main.rs` vs. `crates/renderer/src/vulkan/material.rs` + `docs/engine/memory-budget.md`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D7-2026-08-12-01).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2795
