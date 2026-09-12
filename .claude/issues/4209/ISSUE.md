# PERF-D9-2026-09-11-02: `about_to_wait`'s two screenshot-handshake early returns skip the `atw_*` write

Labels: low,performance,bug

**Description**: Two `return`s (CLI-screenshot claim path, screenshot poll deadline) fire after `render_one_frame` has already run, but before the `atw_*` field write at the handler tail — so those frames print fresh `rof_*`/`between_frames` values next to the *previous* frame's stale `atw_*` values.

**Evidence**:
`byroredux/src/app_events.rs:1254,1279` vs `:1328-1334`.

**Impact**: Confined to `--screenshot`/`byro-dbg`-screenshot frames — rare, but exactly when an operator is capturing evidence; can produce an apparently-impossible `atw_post < rof_pre_draw + rof_draw_call` reading that looks like nesting corruption rather than a skipped write.

**Related**: None named.

**Suggested Fix**: Hoist the three-field write into a helper called before each early return, or restructure to fall through to the tail.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
