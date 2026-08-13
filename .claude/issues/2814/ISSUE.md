# REN-D18-06: resolve_worldspace_climate duplicates inherit_up_chain instead of using the shared helper

Labels: low, renderer, bug

## Description

`e681a3c1` introduced the generic cycle-guarded `inherit_up_chain` and routed DNAM / NAM3+NAM4 / NAM2 through it; **CNAM (climate)** — the one PNAM bit this dimension depends on — kept its bespoke pre-helper loop, duplicating the `visited` cycle guard, the linear form_id reverse lookup, the three `warn!` termination cases and the precedence ordering. Reachable through the generic helper unchanged. A future fix to the shared walk lands in one copy and silently misses climate — the highest-traffic bit, since a missed climate downgrades a whole worldspace to the procedural Mojave fallback sky.

## Location

`byroredux/src/env_translate.rs` (`resolve_worldspace_climate` vs. `inherit_up_chain`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D18-06).

https://github.com/matiaszanolli/ByroRedux/issues/2814
