# #2711: REN-D7-01: `ScratchTelemetry`'s R1 doc block mis-states both the value it holds and the console command that surfaces it

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `crates/core/src/ecs/resources/mod.rs` — the doc comments on
  `ScratchTelemetry::materials_unique` and `materials_interned`. The phantom
  command name recurs twice in `crates/renderer/src/vulkan/material.rs` (a test
  doc comment and one assertion message).
- **Status**: NEW
- **Description**: Two independent inaccuracies in the one doc block a future
  R1 change is meant to be checked against.
  (a) `materials_unique` is documented as "(== `MaterialTable::len()`)", but
  `byroredux/src/main.rs` assigns `self.material_table.unique_user_count()`,
  which deliberately excludes the seeded neutral default at slot 0 (`len() - 1`)
  so the #780 dedup-ratio signal isn't skewed on no-user-material frames. The
  `unique_user_count` doc in `crates/renderer/src/vulkan/material.rs` states
  this correctly, so the two docs contradict each other.
  (b) Both fields are documented as displayed by the `mat.stats` console
  command. No such command exists — `byroredux/src/commands/world_info.rs`
  registers `"ctx.scratch"`, and that handler is what prints the
  `materials: N unique / M interned (R× dedup)` line and the overflow tail.
  This is the same shape as the skill's REN-LOW L-1 / L-6 notes about
  `mem.stats` / `mem`, recurring under a different phantom name.
- **Evidence**:
  ```
  crates/core/src/ecs/resources/mod.rs   "/// (== `MaterialTable::len()`). Pairs with `materials_interned` …"
  crates/core/src/ecs/resources/mod.rs   "/// the `mat.stats` console command. A drop here flags a regression"
  byroredux/src/main.rs                  tlm.materials_unique = self.material_table.unique_user_count();
  byroredux/src/commands/world_info.rs   "ctx.scratch"   ← the only registration
  ```
  `grep -rn '"mat.stats"'` over the tree returns zero registrations; all three
  textual hits are doc comments or an assertion message.
- **Impact**: Documentation only — but this block is the stated contract for
  the dedup-ratio telemetry that exists specifically to catch a silent R1
  regression (alignment hole, non-deterministic float in the producer) before
  VRAM pressure shows it. A reader who cannot find the command, or who derives
  the ratio against the wrong denominator, misreads that signal.
- **Related**: #2273 (the sibling stale field-count in the same subsystem's
  docs — see "Prior-pass reconciliation").
- **Suggested Fix**: Change the `materials_unique` doc to
  "== `MaterialTable::unique_user_count()` (`len()` minus the seeded neutral
  default at slot 0 — see #1032)", and replace all three `mat.stats` mentions
  with `ctx.scratch`.

---

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12.md` (finding `REN-D7-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

