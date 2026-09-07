# #3808 — EX-14/15 item B: full SpeedTree geometry rendering (Phase 2.1 geometry-tail decode)

State: OPEN
Labels: enhancement, renderer, legacy-compat, terrain-exterior, speedtree

Split from #2369 (EX-14/15 item B) per that issue's own plan doc,
[`docs/engine/exterior-readiness-plan.md`](../../docs/engine/exterior-readiness-plan.md#ex-1415-ground-covertrees-persistent-refs-fo4-spatial-data),
which explicitly says to scope this "as its own follow-up issue" per
`exal-trees.md`'s §8 rollout.

## Status (verified 2026-08-31): NOT DONE — billboard-only, confirmed

`crates/spt/src/import/mod.rs` always emits one placeholder billboard
quad, fully wired and tested via `byroredux/src/systems/billboard.rs`.
No real tree geometry has ever been imported.

## Design authority

[`exal-trees.md`](../../docs/engine/exal-trees.md) (PROPOSED, 2026-08-23)
already covers: geometry-tail decode strategy, branch/frond + leaf-card
import shape, the RT/BLAS boundary, wind response (reuses ground cover's
`WindField` — not a second system), and a 5-phase rollout:

- **Phase 2.1 — geometry-tail dissection.** Genuinely unstarted.
  `format-notes.md`'s own log identifies two candidate high-tag markers
  past `tail_offset` and stops there — no vertex/index layout confirmed.
  This is real research-spike work, not assumed-solved by the design doc
  existing. **Start here.**
- Phase 2.2 — branch/frond + static BLAS import once 2.1 confirms the
  layout.
- Phase 2.3 — leaf-card canopy.
- Phase 2.4 — wind response via the shared `WindField`.
- Phase 3 — mid-distance LOD tier (deferred past this issue's scope).

No code lands from the design doc alone — Phase 2.1's byte-layout
confirmation against real `.spt` samples is the actual prerequisite for
everything after it.

## Related
#2369 (parent, split). Design doc: `exal-trees.md`. `docs/engine/format-notes.md`
for the existing partial `.spt` byte-layout notes.
