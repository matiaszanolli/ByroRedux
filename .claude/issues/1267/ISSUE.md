# TD9-NEW-01: byroredux/src/main.rs crossed 2000-LOC ceiling (2162) — splittable today, deferred to watchlist

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/1267

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-24.md` — Dimension 9 (File / Function / Module Complexity)

## Severity
**LOW** — file-complexity ceiling crossed by +162 LOC. No correctness or runtime impact.

## Status
**NEW** at HEAD `8b5d77c1`. **Watchlist** — see deferral criterion below.

## Description
`byroredux/src/main.rs` crossed the workspace 2000-LOC ceiling for the first time at **2162 LOC** (verified `wc -l byroredux/src/main.rs`). The 2026-05-22 tech-debt audit showed only `draw.rs` (2899) and `context/mod.rs` (2661) over the ceiling — main.rs joins them today.

## Driver (2026-05-22 → 2026-05-24)
- M47.0 Phase 1+2 (`6c51af55` Papyrus demo wiring + `a80781a7` ScriptRegistry + defaultRumbleOnActivate spawner)
- M27 Phase 1+2+3 (`a9810d40` + `05fe2bac` system-access declarations across parallel stages — 0 unknown / 0 conflicts)

main.rs is the entry surface; every milestone integration adds wiring here. Unlike the Vulkan-recording carryover (`TD9-200/201`), this file is splittable **today** without a smoke-harness precondition — no command-buffer recording lives in main.rs.

## Suggested split axes
1. `byroredux/src/init/` — engine init (Vulkan context, plugin DataStore, ECS bootstrap, scene loading)
2. `byroredux/src/cli.rs` — argument parsing + `--bsa` / `--esm` / `--cell` / `--grid` / `--bench-*` handling. The piece that grows whenever a new CLI flag lands
3. `byroredux/src/runtime.rs` — per-frame system schedule + winit `ApplicationHandler` wiring
4. `byroredux/src/main.rs` shrinks to a thin top-level dispatch

## Deferral / Promotion Criterion
The audit body deliberately marks this as "Not yet a Top-5 priority" because the file is only +162 over the ceiling and the growth is feature-driven (not pathological).

**Promote when main.rs crosses 2400 LOC** before a split lands. Until then, this issue is **watchlist**: tracked but not actively scheduled.

## Estimated Effort
- **Small-to-medium**. The split surface is mostly init / orchestration, not recording. No `unsafe` blocks, no Vulkan lifecycle that crosses the split boundary, no ECS lock-order entanglement.

## Completeness Checks
- [ ] **UNSAFE**: N/A — no `unsafe` blocks in the candidate split surface
- [ ] **SIBLING**: Verify the split doesn't break `byroredux-core` or any specialist crate that imports from `byroredux::main` (none should — main is the binary entry, not re-exported)
- [ ] **DROP**: N/A — Vulkan objects are owned by `App` and stay co-located; the split moves init code, not `Drop` impls
- [ ] **LOCK_ORDER**: N/A — system-access declarations under M27 are unchanged by the move
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing 336/336 byroredux tests must continue to pass; no new test needed for a pure module-reorg

## Related
- `TD9-200` / `TD9-201` (BLOCKED carry) — `draw.rs` (3233) + `context/mod.rs` (2882). Different gating (RenderDoc harness precondition). This finding is the unblocked sibling.
- #1115 (TD9-001, CLOSED) — `byroredux/src/render.rs` 1306-LOC god-function — historical precedent for a binary-side split.
- #1051 / #1052 / #1056 / #1118 (all CLOSED) — Session-34 / Session-36 file-split sweeps.
