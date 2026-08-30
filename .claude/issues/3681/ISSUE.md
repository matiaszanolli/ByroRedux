# #3681 — PERF-D2-2026-08-30-02: the #2691 note in `render/mod.rs` transcribed two derived counts, and both are now wrong — the exact rot its own last sentence warns against

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D2-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3681

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:818-825`
- **Status**: NEW (the note itself is the #2691 fix; this is fresh drift in it, not a re-file of #2691)
- **Description**: The note reads:
  > "…see the `bench_draws_cmds` column of `.claude/audit-baselines/runtime/*.tsv`, where **exactly one
  > of five cells falls in that band**, **three sit in the 1800–2600 range**, and the FO4 baseline is
  > *above* this gate and takes the parallel path. Cited rather than transcribed, per the audit's
  > cite-don't-copy rule — **a number copied here is a number that rots.**"

  Checked against the five TSVs at HEAD: **zero** cells fall in the quoted 400–1500 band (oblivion 325
  is below it, fo3 1581 is above it), and **two**, not three, sit in 1800–2600 (fnv 2110, skyrim 2342).
  The note avoided copying the raw `bench_draws_cmds` values but copied the *counts derived from them*,
  which rot identically — and did rot, one day after the note landed. The third clause is separately
  unsupported (see PERF-D2-2026-08-30-01).
- **Evidence**: `bench_draws_cmds` at HEAD = 325 / 1581 / 2110 / 2342 / 3949 (table above).
  The counts were **already wrong when written**: `.claude/issues/2691-2692-2695-2696/ISSUE.md:127-135`
  records the then-current column as 324 / 1839 / 2342 / 2553 / 3440 — of which none is in 400–1500
  either, while three were in 1800–2600. `git log -p` on the fo3 TSV shows `bench_draws_cmds` 1839 →
  1581 at `fb21f9ee`, corrected again at `e0a9ee54` (#3407, 2026-08-28), which is what took the
  1800–2600 count from three to two.
- **Impact**: Documentation only, but it is the specific document written to stop the next tuner
  reasoning from a stale distribution — and a reader who checks the "exactly one of five" claim and
  finds it false has no way to tell which of the note's remaining assertions still hold.
- **Related**: #2691 / PERF-D2-03, #3407 (`e0a9ee54`), #3005 (CLOSED at `cc666a48`)
- **Suggested Fix**: Replace both counts with the qualitative statement the evidence actually
  supports — "no baseline cell sits in the quoted 400–1500 band; the median cell is ~2 100 commands"
  — or drop the counts entirely and point at the TSVs, which is what the note's own cite-don't-copy
  rule prescribes.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
