# #4025 — REN-2026-09-06-D23-03: `fsr_bench_report.py` discards the provenance line the harness writes for exactly this purpose, and stamps nothing of its own

**Labels**: low, renderer, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D23-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: FSR/Presentation (debug/telemetry)
- **Location**: `scripts/fsr_bench_report.py` (`main` — the
  `not line.startswith("#")` filter, and the per-scene `print` block);
  `scripts/fsr-bench-matrix.sh` (the `# harness=%s engine=%s …` `printf`)
- **Status**: NEW
- **Description**: #2835 added the `# harness=… engine=… mode=… camera=… runs=…
  frames=…` header to the TSV because "nothing in a committed table said which
  harness produced it". The tool that turns the TSV into the table people
  actually quote drops that line as metadata and never re-emits it, so the
  human-readable output still says nothing about harness or engine commit. The
  reporter also records no version of its own — and it is not a pure formatter:
  `0e91fc5e` changed `render_sum` so brackets named in `gpu_inactive` are
  excluded from the render-resolution sum instead of summed as measured zeros.
  The same TSV therefore yields different "render rec." figures before and after
  that commit, with nothing in the output distinguishing them.
- **Evidence**:
  ```python
  # main(): the provenance line is filtered out and never referenced again
  lines = [line for line in handle if line.strip() and not line.startswith("#")]
  ```
  The only per-scene header printed is
  `f"\n=== {scene} — {mode}/{camera}, {entities} entities, {runs} runs, median (min–max)"`
  — scene state, no provenance. Contrast the harness, which went to the trouble
  of computing `HARNESS_COMMIT` from
  `git log -1 --format=%h -- scripts/fsr-bench-matrix.sh`.
- **Impact**: Bench tables are pasted into `ROADMAP.md` and audit reports. A
  pasted table carries no way to tell which harness/engine produced it or which
  reporter computed its recovery columns — which is precisely the gap that let
  D23-02's stale byte-stability claim survive two folds unchallenged. Purely a
  measurement-hygiene issue, no runtime effect.
- **Related**: #2835, `0e91fc5e` (#2821), REN-2026-09-06-D23-02
- **Suggested Fix**: Echo the `#` provenance line(s) verbatim at the top of the
  report, and add a `report=<git log -1 --format=%h -- scripts/fsr_bench_report.py>`
  token next to them so both halves of the harness pair are stamped. The
  self-test already fixtures the `# harness=deadbeef engine=cafef00d` header, so
  the assertion is a one-line addition to the existing loop.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
