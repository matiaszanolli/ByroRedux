# REN-D12-2026-09-24-02: the #4602 host-flush-edge test's "end_command_buffer must follow" assertion is satisfied by the test's own literal

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D12-2026-09-24-02**._

- **Severity**: LOW (test gap; the edge itself is correct).
- **Dimension**: Sync/Barriers (readback visibility)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` — `mod host_readback_flush_edge_tests::the_host_flush_edge_precedes_end_command_buffer_and_follows_every_writer`.
- **Status**: NEW.
- **Description / Evidence**: `let end = ".end_command_buffer(cmd)".to_string();` is not composed from fragments, unlike `edge`, so `src[edge_pos..].contains(&end)` matches the test's own literal (the needle occurs at production line ~2296 and test line ~3204). `production.contains("presentation")` also matches any comment. Only the `screenshot_record_copy` ordering is real. Nothing pins the property the edge's comment relies on, that it is the *last* recorded command; the ground-cover counter copy, model-tier `stats_readback` copy, depth-capture copy and presentation health atomics are covered only because of that placement.
- **Impact**: A later commit that records a readback writer between the edge and `end_command_buffer` passes the test and reintroduces the #4602 stale-host-read window. This is the same self-matching-needle defect #4604 repaired ~140 lines below in the same file.
- **Suggested Fix**: Compose the `end` needle at runtime like `edge` (#3442), and assert `src[edge_pos..end_pos]` (production text only) contains no `cmd_` / `record_` / `memory_barrier` call.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

