# #3479 — NIF-2026-08-27-D4-01: the SSE triangle-drop diagnostic still names `vertex_map` after #3355 retargeted the bound

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: low, nif-parser, nif, bug, game:skyrim

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 4 (Geometry Extraction & Import Handoff). Severity **LOW**. Game: **Skyrim SE**. Introduced by `07ca5979`, 2026-08-27.

## Location
`crates/nif/src/import/mesh/sse_recon.rs:159-164` (re-verified at publish time: the `vertex_map` wording is still at `:162`).

## Description
#3355 replaced the `vertex_map`-keyed bound with a `decoded.positions.len()` bound (and the surrounding comment says so explicitly — "The #725 drop policy is kept, retargeted at the real bound — the decoded buffer's vertex count"), but the log line 60 lines below still reads:

```rust
"BSTriShape SSE reconstruct: dropped {} triangle(s) with \
 out-of-range vertex_map indices (truncated/malformed NIF)",
```

`vertex_map` is no longer consulted on this path at all.

## Evidence
The drop test is `(global as usize) < vertex_count` at `sse_recon.rs:143-148`, where `vertex_count = decoded.positions.len()` (`sse_recon.rs:133`). No `vertex_map` read remains in the function — the only surviving `vertex_map` mentions in the file are the doc/comment block at `:73-131` and the message itself at `:162`; `remap_bs_tri_shape_bone_indices` keeps its own `vertex_map` reads on a different path.

## Impact
Diagnostic only, and the corpus says the branch never fires on vanilla content (0 drops across 26,913 SSE partitions). But this message is the *only* signal a truncated SSE NIF produces, and it currently points the reader at the wrong data structure.

## Related
#3355 / #725 / NIF-D4-04.

## Suggested Fix
Reword to "out-of-range global vertex indices (past the decoded buffer's vertex count)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other diagnostics in `import/mesh/` naming a bound #3355 or a successor retargeted)
- [ ] **TESTS**: A regression test pins this specific fix, if the drop branch is exercised at all
