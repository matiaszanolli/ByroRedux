# #3890: TD8-2026-09-05-07: Three unused dependencies across three manifests — a fresh crop of the #2426–#2431 / #2075 class

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-07) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3890 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-07), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/Cargo.toml` (`toml`), `crates/ui/Cargo.toml` (`image`), `tools/byro-detect/Cargo.toml` (`byroredux-core`, `toml`)
- **Status**: NEW (four prior sweeps of this class all CLOSED: #2075, #2426, #2427, #2428, #2429, #2430, #2431)
- **Effort**: trivial (≤30 min)

**Description**
`cargo machete` reports three manifests with declared-but-unreferenced dependencies. All three verified by hand — `cargo machete` misses macro-only and re-export usage, so each was re-checked with a `use`/path grep over the whole crate, tests and examples included:

```
$ grep -RInE '(^|[^a-z_])toml::|use toml|extern crate toml' byroredux/       → (empty)
$ grep -RInE '(^|[^a-z_])image::|use image|extern crate image' crates/ui/    → (empty)
$ grep -RInE 'byroredux_core|toml' tools/byro-detect/src/
  tools/byro-detect/src/main.rs:144:    home.join(".byroredux").join("profiles.toml")   # a filename string, not the crate
```

`tools/byro-detect` is the more interesting one: it builds a path to `~/.byroredux/profiles.toml` but never parses or writes it, so both `toml` and `byroredux-core` are declared against an intention rather than a use. `crates/ui`'s `image` is pinned with `default-features = false`, which suggests it was once used for the offscreen wgpu pixel readback before that moved to raw buffers.

**Impact**
`byroredux-core` in `byro-detect` is the costly one — it drags the whole ECS/animation/CHARAL crate into the launcher-detection binary's dependency graph and every CI build of it. `image` and `toml` are smaller but still compile-time-only cost for zero benefit. Nothing breaks; this is pure build-time waste plus a misleading signal about what the launcher tools actually depend on.

**Related**: #2075, #2426–#2431 (all CLOSED — same class, showing it recurs about every 3 months and needs a CI gate, not another sweep)

**Suggested Fix**
Remove all four declarations. Given this is the fifth occurrence, consider adding `cargo machete` to CI (it exits non-zero on findings and runs in seconds) so the class stops re-accruing between audits.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
