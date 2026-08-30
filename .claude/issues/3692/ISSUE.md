# #3692 — PERF-D9-2026-08-30-04: `between_frames` is the only `CpuFrameTimings` field the console `cpu_ms:` line omits — the remainder bucket is invisible to the headless triage surface

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D9-2026-08-30-04`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3692

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/systems/debug.rs:103-124` (`cpu_breakdown`), vs `byroredux/src/systems/metrics.rs:205-226` (the overlay, which does include it)
- **Status**: NEW
- **Description**: `cpu_breakdown` prints thirteen fields —
  `fence_wait acquire submit_present ssbo_build geom_rebuild tlas_build
  cmd_record rof_pre_draw rof_draw_call rof_post_draw atw_pre atw_scheduler
  atw_post` — and omits `between_frames_ms`. That is the one field that is not
  nested inside another printed bucket, i.e. the only one that can expose the
  time the process spends outside `about_to_wait` (compositor throttling,
  Wayland frame-callback wait, event-loop sleep). The egui Metrics panel does
  surface it (`metrics.rs:213`), so the omission is specific to the *console*
  line — which is the surface a `byro-dbg` / `--bench-hold` / headless-log
  operator has, and the one the SLOW-FRAME warning uses.
- **Evidence**:
  ```rust
  // debug.rs:104-124 — the format string, verbatim field list
  "fence_wait={:.0} acquire={:.0} submit_present={:.0} ssbo_build={:.0} \
   geom_rebuild={:.0} tlas_build={:.0} cmd_record={:.0} rof_pre_draw={:.0} \
   rof_draw_call={:.0} rof_post_draw={:.0} atw_pre={:.0} atw_scheduler={:.0} \
   atw_post={:.0}"
  ```
  `metrics.rs:213`: `cpu_pass_ms.insert("between_frames".to_string(), cpu.between_frames_ms);`
  — the field is live and produced, just not printed here.
- **Impact**: `cpu_breakdown`'s own doc (`debug.rs:96-102`) frames the line as
  "the decisive localizer for a multi-second frame whose GPU passes are cheap"
  and enumerates the conclusions it supports — but the "compositor / OS /
  outside-the-engine" conclusion has no bucket on the line to support it. On a
  hitch the operator can localize to `fence_wait` / `atw_post` / `acquire` but
  cannot rule the frame *out* of the engine. Diagnostic-only; LOW. Fixing this
  without finding 01 first would print a number that means the wrong thing.
- **Related**: `PERF-D9-2026-08-30-01` (fix that first); #2183 (the same class of
  omission on the GPU line, for `upscale`/`presentation` — closed).
- **Suggested Fix**: Add `between_frames={:.0}` to `cpu_breakdown`'s format
  string, after finding 01 lands. Consider a one-line note in the doc comment
  that the buckets nest (`atw_post ⊇ rof_* ⊇ …`) so the line is not summed.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
