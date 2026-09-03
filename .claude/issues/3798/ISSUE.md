# #3798 — REG-2026-08-30-08: both manual_bench_draw_sort_* benches panic on integer overflow in a debug build — (i * 2654435761) needs wrapping_mul

**Severity**: LOW · **Location**: `byroredux/src/render/draw_sort_key_tests.rs:505` and `:602`
**Source**: `docs/audits/AUDIT_REGRESSION_2026-08-30.md` (REG-2026-08-30-08)

Both benches computed `c.mesh_handle = (i as u32 * 2654435761) & 0xFFFF;` — a plain `*` that
overflows for `i >= 2` and panics under `debug_assertions`.

**Suggested Fix**: `wrapping_mul(2654435761)` at both sites.

**Investigation result**: STALE PREMISE. Both sites at HEAD already read
`(i as u32).wrapping_mul(2654435761) & 0xFFFF` — fixed by an earlier, unrelated commit
(`c604375f`, "Refactor material overflow handling and improve water shader functionality").
Sibling check (the issue's own checklist item) swept every other `2654435761` hash-mix site
tree-wide (`particle.rs:291`, `particle.rs:381`, `attach.rs:825`) — all three already use
`wrapping_mul` too. No code change needed; closed with citation.
