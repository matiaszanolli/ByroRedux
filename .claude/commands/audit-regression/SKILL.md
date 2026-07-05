---
description: "Verify closed bug fixes haven't regressed — dynamically discovers and checks"
argument-hint: "--issues <N,N,N> --limit <N> --label <label>"
---

# Regression Verification Audit

Confirm that previously-fixed bugs are still fixed. This audit **dynamically
discovers** closed bug issues from GitHub, locates each fix and its guard test,
and reports any fix that has gone missing as a **Regression of #NNN**.

See `.claude/commands/_audit-common.md` for project layout, severity, dedup,
context rules, and the per-finding format. See `.claude/commands/_audit-severity.md`
for the severity scale. This file only adds the regression-specific flow.

## Parameters (from $ARGUMENTS)

- `--issues <N,N,N>`: Verify only these issue numbers (e.g., `--issues 9,16,1516`).
- `--limit <N>`: Max closed issues to verify (default: 50).
- `--label <label>`: Issue label filter (default: `bug`).

## Step 1 — Discover fixed issues

```bash
gh issue list --repo matiaszanolli/ByroRedux --state closed --label bug \
  --limit 50 --json number,title,body,closedAt,labels
```

With `--issues`, fetch those numbers directly (`gh issue view <N> --repo
matiaszanolli/ByroRedux --json number,title,body,closedAt,labels`) instead.

> Default `--label bug` structurally misses closed issues carrying only the
> `documentation` label (e.g. #1818, a doc-rot fix) — pass
> `--label bug,documentation` to include doc-rot regressions in the discovery pass.

For each issue, pull out:
- **Number + title** — the regression handle.
- **File references** — backtick-quoted paths in the body (`crates/nif/...`).
- **Acceptance criteria / fix description** — what the fix is supposed to do.
- **Related `#NNNN`** — phased fixes split across several issues (e.g. #1210 →
  #1255 → #1257) regress as a set; verify the whole chain, not just the head.

> **Discovery window caveat.** The repo has 1600+ closed issues. The default
> `--limit 50` only covers the most-recently-closed bugs, so older high-value
> fixes get **no coverage** unless you raise `--limit` or pass them via
> `--issues`. The unconditional **Step 4** fragile-area checks are the safety
> net for fixes that landed as proactive refactors and were never an issue at
> all — run them every time regardless of which issues Step 1 surfaced.
>
> **Fresh verification candidates (recent decompiler-safety + LC wave).**
> Recently-closed, high-churn fixes worth an explicit `--issues` pass while
> they're still warm: #1815 (decompiler recursion-depth cap in the boolean-collapse
> pass), #1816 (`translate_pex` missing `catch_unwind`), #1728 (Skyrim-BE/Starfield
> round-trip test for the `.pex` reader), #1740 (DA10 `.pex` byte-equality parity
> test), #1731 (VWD record-header flag parse + expose), #1718 (ragdoll
> bone/constraint-drop telemetry on bone-name miss). Note **#1651** (BGSM/BGEM
> GL→Gamebryo blend factors) was itself a WRONG fix — its premise was disproven and
> reverted by **#1823**; don't re-verify #1651 as if it still holds. Several of
> these touch the import→material boundary that **Step 4** already pins —
> cross-check there.

## Step 2 — Locate each fix and its guard

For each issue, work the fix → guard-test chain:

1. **Find the fix commit.** `git log --oneline --grep="#<N>"` (commits use
   `Fix #<N>: …` and `fix/<N>-…` branch merges). `git show <commit> --stat`
   shows which files moved.
2. **Confirm the fix is present** in the live tree. Read the referenced file(s)
   at the symbol named in the issue/commit (prefer `grep -n "fn <name>"` over a
   line number — the post-Session-34/35 module splits invalidate old line refs).
3. **Find the guard test.** Tests live as `*_tests.rs` siblings next to the
   module they cover (e.g. `crates/nif/src/blocks/interpolator_tests.rs`), or as
   `#[cfg(test)] mod tests` inline. To locate one:
   - `grep -rn "<N>" crates/ byroredux/ --include='*.rs'` — many tests cite the
     issue number in a name or comment (`fn fix_1516_…`, `// #1516`).
   - Failing that, grep the fixed symbol or a keyword from the title across
     `*_tests.rs` siblings of the fix file.
4. **Run the guard** to prove it still passes. Crate packages are named
   `byroredux-<crate>` (e.g. `cargo test -p byroredux-nif <test_name>`,
   `cargo test -p byroredux-renderer`, `cargo test -p byroredux-core`).

## Step 3 — Assign a status

- **PASS** — fix code confirmed present **and** a guard test exists (ideally run green).
- **PARTIAL** — fix code present but **no** guard test. Flag as a hardening gap.
- **FAIL** — fix code missing or its guard now fails. **This is the regression.**
  Report it with `Status: Regression of #<N>` per the `_audit-common` finding format.
- **UNVERIFIABLE** — the issue body names no file/symbol and no fix commit is
  findable. Note it and move on; don't guess.

## Step 4 — Unconditional fragile-area checks

These guard fixes/contracts whose breakage is **invisible to GitHub-issue
discovery** — most landed as refactors, not closed bugs — so check them every
run regardless of Step 1's window. A FAIL here is still reported as a regression
(reference the relevant issue if one exists, else describe the contract).

**NIFAL canonical-translation tier** (spec: `docs/engine/nifal.md`; see also
`/audit-nifal` for the dimension-level checklist):

- **Single material boundary.** `byroredux/src/material_translate.rs` (`fn
  translate_material`) must remain the *only* `ImportedMesh → Material` site —
  per-game material classification lives here, never in a shader. `Material`
  (`crates/core/src/ecs/components/material.rs`) `metalness` / `roughness` must
  stay plain resolved `f32` fields — no reintroduced `Option<f32>` and no
  render-time classifier. The resolve-once contract is the boundary filling
  overrides + `Material::resolve_pbr` (which calls `classify_pbr_keyword`)
  filling only the unresolved slots.
- **Typed particle emitters.** `NiPSysEmitter` / `NiPSysEmitterCtlr` /
  `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` must still parse as **typed**
  blocks (`crates/nif/src/blocks/particle.rs`, dispatched in
  `crates/nif/src/blocks/mod.rs`), feed `extract_emitter_params` /
  `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs` →
  `ImportedEmitterParams` in `crates/nif/src/import/types.rs`), and be consumed
  by `apply_emitter_params` (`byroredux/src/systems/particle.rs`). A regression
  to opaque `NiPSysBlock` shows up as zero-sized emitters or clobbered colors.
- **Collision shape coverage.** `BhkMultiSphereShape` + `BhkConvexListShape`
  must still translate to a `CollisionShape` in
  `crates/nif/src/import/collision/mod.rs` (they were previously dropped to `None`).

**Disney BSDF + GPU struct contracts** (recent shader wave):

- The Disney/Burley lobe now lives in `crates/renderer/shaders/include/pbr.glsl`
  (split out of `triangle.frag`; the GLSL-PathTracer MIT attribution block stays
  top-of-`triangle.frag`, Burley 2012 cite). The per-reservoir `resRadiance[]`
  array was retired (#1369 factoring → commit 218b425b, which removed the ReSTIR
  reservoir G-buffer attachment): WRS is register-local now, recomputing the
  unshadowed radiance from the light index via `shadowableLightRadiance` in
  `crates/renderer/shaders/include/lighting.glsl`. A regression here is a
  reintroduced per-thread reservoir array or a re-added G-buffer reservoir
  attachment — verify the array stays gone, not "intact".
- `#[repr(C)]` GPU structs hold their size pins in
  `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`:
  `GpuInstance` = 112 B (`gpu_instance_is_112_bytes_std430_compatible`) and
  `GpuCamera` = 336 B (`gpu_camera_is_336_bytes`). Run them:
  `cargo test -p byroredux-renderer gpu_`.

## Output

Write to: **`docs/audits/AUDIT_REGRESSION_<TODAY>.md`** (YYYY-MM-DD).

### Per-issue entry

```
## #<ISSUE>: <Title>
- **Status**: PASS | PARTIAL | FAIL | UNVERIFIABLE
- **Closed**: <date>
- **Fix commit**: <hash> (or "not found")
- **Fix site**: `<path>` (`<symbol>`)
- **Fix present**: Yes / No / Unknown
- **Guard test**: `<test name>` in `<path>` — passes / fails / none
- **Notes**: <concerns>
```

### Summary table

```
| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
```

For any **FAIL**, surface it as a `Regression of #NNN` finding (base format in
`_audit-common.md`) and suggest:
`/audit-publish docs/audits/AUDIT_REGRESSION_<TODAY>.md`
