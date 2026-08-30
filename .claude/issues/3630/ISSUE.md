# #3630 — REN-2026-08-30-D20-05: `depth.stats` contradicts `analyze_depth_field`'s explicit degenerate-camera contract

**Labels**: `low,renderer,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3630 --json state`.

---

- **Severity**: Trivial
- **Dimension**: Debug/Telemetry
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`)
- **Status**: Open
- **Description**: `analyze_depth_field` returns early on `self.near <= 0.0 || self.far <= self.near` with `total` populated but `cleared == 0`, `invalid == 0`, `bands` empty — a contract its own test pins as "must report nothing rather than … emit bands it cannot justify" (`camera.rs:602-608`). The command does not honour that: it computes `geometry = stats.total - stats.cleared - stats.invalid`, which on that path equals the full sample count, and *also* prints "(no geometry in frame — every sample is background)" because every band has `samples == 0`. The two lines contradict each other and neither says the camera was rejected.
- **Evidence**:
  - `camera.rs:322-325`: `if self.near <= 0.0 || self.far <= self.near { return stats; }` with `stats.total` already set from `encoded.len()`.
  - `depth.rs`: `stats.total - stats.cleared - stats.invalid` for the `geometry=` field, then `if stats.bands.iter().all(|b| b.samples == 0)` → "(no geometry in frame …)".
  - Reachability is narrow: it needs `near <= 0` or `far <= near` on the live `Camera`, which no CLI flag or `FOV_SETTING_ID` slider produces. Filed as Trivial for that reason, not because the mismatch is theoretical — the contract is explicit and tested on the analytic side.
- **Impact**: A misconfigured camera reports a full frame of "geometry" with no bands, which reads as a broken readback rather than a rejected camera — the opposite of what `analyze_depth_field`'s doc says a disagreeing capture means.
- **Suggested Fix**: In `execute`, short-circuit on `stats.bands.is_empty() && stats.total > 0` with an explicit "degenerate camera (near={}, far={}) — analysis rejected" line before the per-band table.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D20-05

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
