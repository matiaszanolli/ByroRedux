# FNV-2026-08-26-D8-05

**Issue**: #3348
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 8 — Real-Data Validation & Bench
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/bsa/src/lib.rs:10-18`, `crates/bsa/src/ba2.rs:49-52`

**Premise verified**: both blocks are ```` ```ignore ````-fenced usage sketches that
use `?` inside an implicit `fn main() -> ()`. Under `-- --ignored` the doc harness
*runs* them and they fail to compile:

```
test crates/bsa/src/lib.rs  - (line 10)   ... FAILED
test crates/bsa/src/ba2.rs  - ba2 (line 49) ... FAILED
test result: FAILED. 0 passed; 2 failed; …
error: doctest failed, to rerun pass `-p byroredux-bsa --doc`
```

`cargo test --workspace --doc -- --ignored` shows these are the workspace's only two.
The BSA crate's *real* ignored on-disk tests are all green:
`cargo test -p byroredux-bsa --lib -- --ignored` → **11 passed, 0 failed**.

**Impact**: the `--ignored` sweep is the prescribed way to exercise real game data
(BRIEF, and every per-game audit skill). It currently exits non-zero on the reference
title's archive crate for a reason unrelated to any archive, which trains readers to
ignore a red `--ignored` run — precisely where a genuine BSA regression would surface.

**Fix sketch**: retag both as ```` ```text ```` (they are illustrative, not runnable),
or convert to `no_run` with an explicit `fn main() -> Result<(), Box<dyn std::error::Error>>`.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
