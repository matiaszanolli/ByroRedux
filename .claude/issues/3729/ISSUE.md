# #3729 — ESM-2026-08-30-D7-03: ESM parse cost is 1.2-3.4s per master, no owner audit

**Severity**: LOW · **Location**: `crates/plugin/src/esm/records/mod.rs` (`parse_esm_with_load_order`)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D7-03)

ESM parse cost (1.2-3.4s per master, single-threaded, on the critical path
of the first cell load) had no owner audit — `/audit-performance` Dim 8
owned NIF parse cost but not this. The issue's own suggested fix was two
parts: (1) adopt the cost into Dim 8's ownership, (2) scope parallel
per-plugin parsing.

## Fix implemented

**Part 1 (done here)**: `.claude/commands/audit-performance/SKILL.md`'s
Dimension 8 is renamed "NIF + ESM Parse Performance", gains
`parse_esm_with_load_order` as an entry point, and a new checklist bullet
carrying the issue's own measured table (`Oblivion.esm` 265MB/1.41s through
`SeventySix.esm` 880MB/3.41s) plus the architectural note (per-plugin
`FormIdRemap` is header-only, `merge_from` is the sole join point — parallel
parsing is *possible*, not yet *implemented*, and any finding proposing it
must design around #3403/#3384's ordering guarantees and add a
byte-identical-`EsmIndex` regression test rather than just parallelizing).
Ran `.claude/commands/_audit-validate.sh` after the edit — `OK: all path
references valid`, no drift introduced.

**Part 2 (deliberately NOT implemented here, filed separately as #3813)**:
parallelizing the per-plugin walk itself is a genuine architecture change
with real correctness risk — `merge_from`'s ordering semantics need to be
*verified* preserved (not assumed) under a parallel-parse-then-sequential-
fold design, and any new shared state would need to extend the project's
TypeId-sorted lock-acquisition invariant. This doesn't fit a LOW-severity
single-issue fix: it needs its own design pass and a real regression harness
(the issue's own TESTS checklist item — "a parallel parse produces a
byte-identical `EsmIndex`" — presupposes the parallel implementation exists,
which it doesn't yet). Filed as #3813 with the design sketch (compute every
remap up front, parse each plugin on a worker into its own `EsmIndex`, fold
sequentially in load order exactly as today) so the work isn't lost, rather
than attempting a speculative concurrency change under an audit-hygiene
issue's low-risk budget.

**SIBLING / LOCK_ORDER / TESTS** (issue's own checklist items): all three
are scoped to "if parsing is parallelized" — since Part 2 isn't implemented
in this fix, they're inherited by #3813 rather than addressed here.

No code paths changed; this is a documentation-only fix. Full workspace:
`cargo test --no-fail-fast` unchanged at 7056 passing, 0 failing (no source
touched).
