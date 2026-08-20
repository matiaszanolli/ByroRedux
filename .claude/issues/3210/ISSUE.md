# REG-2026-08-20-D1-01: esm/records/tests.rs is a binary file to grep - 40 guards citing 31 issues invisible

**Issue**: #3210 — https://github.com/matiaszanolli/ByroRedux/issues/3210
**Severity**: MEDIUM
**Labels**: `medium,import-pipeline,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-20.md` § REG-2026-08-20-D1-01 (Dimension 1 — Closed-issue discovery & fix presence).

**Severity**: MEDIUM · **Effort**: trivial to fix; the value is in understanding what it costs
**Location**: `crates/plugin/src/esm/records/tests.rs:1678`, `:1686`, `:1691` — three raw `NUL` (`0x00`) bytes. Introduced by `09682c71` (2026-08-15, *"feat: Implement inventory management and UI integration"*).

## This is not a test that is green by construction. It is a *tool* that is blind — and it blinds all 27 audit skills at once.

Every one of the repo's 27 audit skills prescribes the same discovery recipe. `audit-regression/SKILL.md` Step 2.3:

> `grep -rn "<N>" crates/ byroredux/ --include='*.rs'` — many tests cite the issue number

and `_audit-common.md`'s context rule is *"grep before read."*

Against this one file, **both return empty**. GNU `grep` classifies any file containing a `NUL` as binary and, without `-a`, emits `Binary file … matches` at best and — with `--binary-files=without-match`, the effective default in this environment — **silently skips it**.

## What is hidden

`crates/plugin/src/esm/records/tests.rs` is **1,944 lines** holding **40 `#[test]` functions** whose comments cite **31 distinct issue numbers**:

```
#442 #443 #445 #448 #458 #519 #624 #629 #630 #631 #634 #808 #809 #810 #817
#896 #966 #969 #989 #1277 #1304 #1538 #1568 #1666 #1773 #2081 #2908 #2986
#3093 #3094 #3095
```

Two of those guards — **#2908** (2026-08-18) and **#3095** (2026-08-19) — landed **after** the file went binary, so they have **never been visible to a plain grep at any point in their life**.

## Evidence — this audit hit it live and came one command from publishing a false FAIL

```
$ grep -rn "real_ruleset_falsifiability" --include='*.rs' .      # → nothing
$ grep -c falsifiability crates/plugin/src/esm/records/tests.rs  # → rc=1, no output
$ grep -an "mod real_ruleset" crates/plugin/src/esm/records/tests.rs
56:mod real_ruleset_falsifiability {                              # ← it is there
$ file crates/plugin/src/esm/records/tests.rs
crates/plugin/src/esm/records/tests.rs: data
```

The most alarming lead of the sweep was *"the water mega-refactor `73896726` deleted #3095's 146-line guard."* **Three independent signals** pointed at a silent revert: `git show f4e731f6:…tests.rs | grep` returned nothing; `git log -1 -- …tests.rs` resolved to a *water* commit; and the file was 4 lines shorter than the post-fix version.

Blob inspection disproved all three. The commit's only change to that file is a `cargo fmt` line-join; the guard is at `:56`:

```
git cat-file -p c9933353 | wc -l  = 1801
       → 0decee23 (post-#3095)    = 1947
       → 7a304403 (HEAD)          = 1943   (−4 = cargo fmt line-join in 73896726)
Per-commit NUL count crosses 0 → 3 at 09682c71.
```

**Verifying the disproof is what produced this finding.** A less paranoid run publishes the FAIL.

The three bytes are inside byte-string literals — `b"Long Barrel<NUL>"`, `b"Desk Fan<NUL>"`, `b"Overdue Book<NUL>"` — where the source almost certainly intended the two-character escape `\0`. They are valid Rust and compile fine, so **nothing in the build complains**.

Repo-wide, this is the only such file:

```
$ git ls-files -z '*.rs' '*.md' '*.glsl' '*.vert' '*.frag' '*.comp' '*.sh' '*.toml' \
    | xargs -0 -I{} sh -c 'n=$(tr -dc "\000" < "{}" | wc -c); [ "$n" -gt 0 ] && echo "$n NUL  {}"'
3 NUL  crates/plugin/src/esm/records/tests.rs
```

`rg` and `git grep` **do** see it (both return 2 hits). Only plain `grep` does not — which is precisely the tool every skill's recipe names.

## Impact

Against this file, the honest conclusion a **recipe-following** auditor reaches is:

- **"fix present, no guard"** — a PARTIAL where the truth is PASS; or worse
- **"guard deleted"** — a FAIL, when the fix commit's `--stat` says a test file grew by 146 lines and the symbol cannot be found.

The blast radius is **not** limited to `/audit-regression`. It is every audit in the suite that greps for a symbol — all 27 skills prescribe the recipe — plus `/audit-publish`'s own validation step and the session-close symbol-drift gate. `/audit-esm` and `/audit-character` both own material in this file.

The 2026-08-16 sweep's `REG-D5-02` / `REG-D5-03` are the same family (*a guard that exists but cannot do its job*), but this is a new shape that report had no category for: **a guard can fail by being undiscoverable.**

## Suggested Fix

1. Replace the three raw `NUL` bytes with the `\0` escape — `b"Long Barrel\0"` etc. The compiled byte strings are **byte-identical**, so no test changes meaning.
2. Add a cheap tripwire — a CI step or an `_audit-validate.sh` clause — that **fails on any tracked `*.rs` / `*.md` / shader source containing a `NUL`**, so the next one is caught at the commit that introduces it rather than five days and two audit sweeps later.
3. Consider standardising the audit skills' discovery recipe on `rg --text` / `grep -a`, which are immune to this class regardless.

## Related

- **#2908**, **#3095**, **#2986**, **#3093**, **#3094** — guards living inside the blind spot
- `09682c71` — introduced it · `73896726` — the `cargo fmt` line-join that completed the false-revert illusion
- `AUDIT_REGRESSION_2026-08-16.md` § `REG-D5-02` / `REG-D5-03` — same family, different shape
- The `_audit-validate.sh` symbol-advisory blind spots filed from `AUDIT_TECH_DEBT_2026-08-20.md` — the other half of "the verification tooling reports clean because it cannot see"

## Completeness Checks
- [ ] **SIBLING**: All three `NUL` bytes replaced; `file crates/plugin/src/esm/records/tests.rs` reports text, and `grep -rn "#3095" crates/ --include='*.rs'` returns the guard
- [ ] **TRIPWIRE**: A gate fails on any tracked text source containing a `NUL`, so this cannot recur silently
- [ ] **RECIPE**: The audit skills' grep recipe is either NUL-immune (`rg --text` / `grep -a`) or the tripwire is enforced in CI
- [ ] **TESTS**: All 40 tests in the file still pass — the byte strings must compile identically
