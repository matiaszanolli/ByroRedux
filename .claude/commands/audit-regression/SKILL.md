---
description: "Verify closed bug fixes haven't regressed — dynamically discovers and checks"
argument-hint: "--issues <N,N,N> --limit <N> --label <label> --recent"
---

# Regression Verification Audit

Confirm that previously-fixed bugs are still fixed. This audit **dynamically
discovers** closed bug issues from GitHub, locates each fix and its guard test,
and reports any fix that has gone missing as a **Regression of #NNN**.

Read `_audit-common.md` (dedup, methodology, per-finding format) and `_audit-severity.md` for shared protocol. This file only adds the regression-specific flow.

## Parameters (from $ARGUMENTS)

- `--issues <N,N,N>`: verify exactly these issues (skips discovery).
- `--limit <N>`: max issues to verify (default 40).
- `--label <label>`: issue label filter for the fallback pass (default `bug`; use `bug,documentation,doc-rot` to include doc-rot fixes, or `game:<title>` to scope to one title).
- `--recent`: skip churn weighting; take the N most-recently-closed issues instead.

## Step 1 — Discover fixes worth re-checking (churn-weighted)

A fix can only regress if the code around it changed. With 4,400+ closed issues, "the last 50 closed" re-checks fixes nobody has touched and never reaches old fixes in hot files. Select by churn instead:

```bash
mkdir -p /tmp/audit
D=$(ls docs/audits/AUDIT_REGRESSION_*.md | sort | tail -1 | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}')   # last sweep
base=$(git rev-list -1 --before="$D 23:59" HEAD)
git diff --name-only "$base"..HEAD -- '*.rs' '*.glsl' '*.comp' '*.frag' '*.vert' > /tmp/audit/churn.txt
# earlier fix commits on the files that changed since, ranked by overlap:
xargs -a /tmp/audit/churn.txt -I{} git log "$base" --format=%s -- {} \
  | grep -oiE '(fix|fixes|fixed|close[sd]?|resolve[sd]?) #[0-9]+' | grep -oE '[0-9]+' \
  | sort | uniq -c | sort -rn | head -"${LIMIT:-40}" > /tmp/audit/candidates.txt
```

Take the top candidates, then `gh issue view <N> --repo matiaszanolli/ByroRedux --json number,title,body,closedAt,labels` for each. If the churn list is empty or short, top up with `gh issue list --repo matiaszanolli/ByroRedux --state closed --label bug --limit 50 --json number,title,body,closedAt,labels`. Never trust a hand-typed closed-issue count; ask the API.

For each issue pull out: **number + title**; **file references** (backticked paths in the body); the **fix description / acceptance criteria**; **related `#NNNN`** — phased fixes (e.g. #1210 → #1255 → #1257) regress as a set, so verify the whole chain. Note **#1651** was itself a wrong fix, disproven and reverted by #1823 — do not verify it as if it still holds.

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
   - `grep -arn "<N>" crates/ byroredux/ --include='*.rs'` — many tests cite the
     issue number in a name or comment (`fn fix_1516_…`, `// #1516`).
     **The `-a` is load-bearing** (#3210): a single raw NUL byte anywhere in a
     `.rs` file — trivially introduced by writing `b"Foo<NUL>"` where the source
     meant `b"Foo\0"`, and accepted silently by rustc — makes plain `grep`
     classify the whole file as binary and skip it without a word. That once hid
     40 guards citing 31 issues in one 1,944-line file, and the sweep that found
     it came one command from filing a FAIL against a fix that was present.
     `rg` and `git grep` are immune; plain `grep` is not.
     `scripts/check-text-source-integrity.sh` (run in CI, and by
     `_audit-validate.sh`) now rejects such files, but keep the `-a`: a
     PARTIAL/FAIL published on a blind grep costs far more than a flag.
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

## Step 4 — Unconditional fragile-area guards

These contracts landed as refactors, not closed bugs, so issue discovery never surfaces them. Run every time; a failure is reported as a regression (cite the issue if one exists, else the contract). Deep checklists live in the owning audit — this step only proves the guards are live.

| Area | Contract | Verify |
|---|---|---|
| GPU struct sizes | every `#[repr(C)]` shader-contract struct keeps its pinned size (the pins, not this row, hold the numbers) | `cargo test -p byroredux-renderer gpu_` (`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`, `crates/renderer/src/vulkan/material_tests.rs`) |
| NIFAL single boundary | `translate_material` in `byroredux/src/material_translate.rs` is the only `ImportedMesh → Material` site; `metalness`/`roughness` stay plain resolved `f32`; `Material::resolve_pbr` fills only unresolved slots | `cargo test -p byroredux-core resolve_pbr`; `/audit-nifal` |
| Typed particle emitters | `NiPSysEmitter*` parse typed (`crates/nif/src/blocks/particle.rs`) → `extract_emitter_params` (`crates/nif/src/import/walk/emitter.rs`) → `apply_emitter_params` (`byroredux/src/systems/particle.rs`); an opaque `NiPSysBlock` shows as zero-sized emitters | `cargo test -p byroredux apply_emitter_params` |
| Collision coverage | every `Bhk*Shape` parser maps to a `CollisionShape` in `crates/nif/src/import/collision/shape.rs` (incl. `BhkMultiSphereShape`, `BhkConvexListShape`) | `cargo test -p byroredux-nif collision` |
| ReSTIR reservoir | the per-thread `resRadiance[]` array stays *retired* (#1369) — WRS is register-local, radiance recomputed via `shadowableLightRadiance` in `crates/renderer/shaders/include/lighting.glsl`; verify it stays gone, not "intact" | grep `resRadiance` in `crates/renderer/shaders/` → only the retirement comments |
| Scheduler access | every parallel system declares what it acquires | `cargo test -p byroredux system_access_declaration_tests` |
| Save shape | serialized shape changes force a baseline refresh or `FORMAT_MAJOR` bump | `cargo test -p byroredux serde_default_guard_tests` |

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
