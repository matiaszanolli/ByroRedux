# #3586 — REN-2026-08-30-D5-03: `memory-budget.md`'s Texture Registry descriptor-pool row omits the ×2 for the second binding, and the code's own SAFETY comment repeats the omission

**Labels**: `low,renderer,memory,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3586 --json state`.

---

- **Severity**: Low
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md:421`;
  `crates/renderer/src/texture_registry.rs:434-447` and `:1735-1740`
- **Status**: Open — the **doc** (and one code comment) is wrong; the code is right.
- **Description**: The ledger records the bindless descriptor pool as
  `max_textures × MAX_FRAMES_IN_FLIGHT` combined image samplers. Both pool-creation
  sites size it as `max_textures * 2 * MAX_FRAMES_IN_FLIGHT`, deliberately — each
  per-frame set carries **two** `max_textures`-sized bindings, as the line-433
  comment says ("two bindings in each per-frame set"). The `SAFETY` comment three
  lines below the sizing (`:445-447`) then contradicts it: "sizes cover exactly
  `MAX_FRAMES_IN_FLIGHT` sets of `max_textures` samplers each" — the same dropped
  ×2 as the doc, sitting directly under the correct expression.
- **Evidence**:
  - `texture_registry.rs:436-438`: `descriptor_count: max_textures * 2 * MAX_FRAMES_IN_FLIGHT as u32,`
  - `texture_registry.rs:1735-1737`: identical expression on the pool-rebuild path.
  - `docs/engine/memory-budget.md:421`: `| Descriptor pool | max_textures × MAX_FRAMES_IN_FLIGHT combined image sampler descriptors |`
- **Impact**: Halves the documented descriptor-pool ceiling for the one subsystem
  whose known failure mode is exhausting that ceiling (the same section documents
  #2030's grow-only slot leak). A reader sizing `max_textures` against the doc
  under-provisions by 2×.
- **Suggested Fix**: Change the doc row to `max_textures × 2 × MAX_FRAMES_IN_FLIGHT`
  and say why (two bindings per set), and fix the `SAFETY` comment at
  `texture_registry.rs:445-447` to match the expression it is justifying.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D5-03

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
