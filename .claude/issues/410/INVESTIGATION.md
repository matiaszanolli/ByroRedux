# Issue #410 — Investigation

## Conclusion: already fixed, close as duplicate of #390

`crates/plugin/src/legacy/fo4.rs` **does not exist** in the tree today:

```
$ ls crates/plugin/src/legacy/
mod.rs
```

Commit `0cb9352` "Fix #390: delete dead legacy ESM parser stubs" (author:
Matias Zanolli, 2026-04-17) deleted all four stubs in a single change:

```
crates/plugin/src/legacy/fo4.rs  | 14 --------------
crates/plugin/src/legacy/mod.rs  | 14 +++++++++-----
crates/plugin/src/legacy/tes3.rs | 14 --------------
crates/plugin/src/legacy/tes4.rs | 14 --------------
crates/plugin/src/legacy/tes5.rs | 14 --------------
```

The rationale is preserved in the current `legacy/mod.rs:26-32`:

> There used to be `tes3` / `tes4` / `tes5` / `fo4` submodules here, but
> they were `todo!()` stubs with no callers — the working ESM path
> lives in [`crate::esm`] and never produces [`Record`] bundles. The
> plumbing for converting parsed records into the stable `Record` form
> will land alongside its first real consumer; until then the stubs
> were just misleading. See #390.

## No callers exist

```
grep -rn "legacy::fo4\|legacy::tes3\|legacy::tes4\|legacy::tes5\|fo4::parse\|tes5::parse" crates/ byroredux/src/
```

Returns zero matches. Nothing in the workspace imports or references any
of the four removed stubs. The real ESM path (`esm::parse_esm` +
`esm::cell::parse_esm_cells`) handles FO4 via `EsmVariant::Modern`.

## Related open issues

- **#368** (tes5.rs stub) — same pattern, same resolution via #390.
  Should also be closed as duplicate.

## Action

- Close #410 with a pointer to commit `0cb9352`.
- Close #368 as well — same story, same fix.
- No code change needed.
