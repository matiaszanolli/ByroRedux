# TD8-2026-08-16-01: 20 of 25 ALIAS_FLAG_* constants are unreachable outside their own test module; the 5 that are reachable carry a redundant allow(dead_code)

**Issue**: #2982
**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Labels**: `low,import-pipeline,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 8 — Dead Code & Backwards-Compat Cruft). Effort: trivial.

**Location**: `crates/plugin/src/esm/records/misc/quest.rs`:220-286 · re-export at `crates/plugin/src/esm/records/misc.rs`:73-77
**Age**: `a844c26b`, 2026-08-07

## Description

`mod quest;` is private (`misc.rs`:39) and its `pub use` block re-exports exactly **five** of the twenty-five `ALIAS_FLAG_*` constants — `RESERVES`, `ALLOW_REUSE`, `ALLOW_DEAD`, `ALLOW_RESERVED`, `CLOSEST` — which are the five `crates/scripting/src/scene/quest_alias.rs` consumes.

The other **twenty are not re-exported**, so no code outside `quest.rs` can name them; their only use in the whole tree is the `ALL_FLAGS` array in that file's own test module.

Symmetrically, the five that *are* re-exported are reachable through a `pub use` chain and therefore **cannot trip the `dead_code` lint at all** — their `#[allow(dead_code)]` is inert.

So the block carries 25 attributes of which 5 do nothing and 20 mark genuinely unreachable data.

## Evidence

```
$ grep -c 'pub const ALIAS_FLAG_' crates/plugin/src/esm/records/misc/quest.rs
25
$ grep -o 'ALIAS_FLAG_[A-Z_]*' crates/plugin/src/esm/records/misc.rs | sort -u | wc -l
5
$ grep -rn 'ALIAS_FLAG_' crates byroredux | grep -v 'records/misc/quest.rs'
# → only misc.rs / records/mod.rs re-exports and scripting/scene/quest_alias{,_tests}.rs
```

Re-verified 2026-08-16: 25 declared, 5 re-exported.

The block comment claims the catalog *"stays parser-owned"* and *"exposes remaining authored metadata for later gameplay components"* — accurate as intent, but **no consumer can currently reach that metadata**.

## Impact

Low. The values are correct-shaped parsed protocol data and deleting them would be wrong.

The cost is 25 lines of attribute noise and a misleading signal that the catalog is available to consumers when 80% of it is not.

## Suggested Fix

Widen the `pub use` to re-export all twenty-five. They are a protocol catalog, and the crate is workspace-internal so there is no API surface cost — and this removes the need for any `allow` at all.

Failing that, collapse to one module-level `#![allow(dead_code)]` with the existing comment.

## Related

- TD9-2026-08-16-01 / #2983 (the guard that is supposed to exercise them)
- #1761 (TD8-004, OPEN — the same "attribute outlived its need" shape in `Dx10Chunk::start_mip`)

## Completeness Checks
- [ ] **SIBLING**: Other `misc/` record modules checked for the same partial-re-export + blanket-`allow` shape
- [ ] **NO-DELETE**: The 20 unreachable constants are parsed protocol data — widened or annotated, never removed
- [ ] **ATTRIBUTE-TRUTH**: No inert `allow(dead_code)` left on the five reachable constants
- [ ] **TESTS**: `cargo test -p byroredux-plugin` green; if re-exported, the parity check from #2983 covers all 25

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2982 --json state` when live state is needed.*
