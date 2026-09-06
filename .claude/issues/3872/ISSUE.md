# #3872: TD4-2026-09-05-03: two audit SKILL files still name `compute_blas_budget`, renamed hours ago by `fa5c4191`; one pins a stale line anchor, the other states the pre-#3839 formula

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-03) via `/audit-publish`, 2026-09-05. Labels: `low,renderer,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3872 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-03), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-fnv/SKILL.md:84`, `.claude/commands/audit-performance/SKILL.md:122`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `fa5c4191`, 2026-09-05 (today) — fixed #3829/#3839/#3840 and split the function.

**Description**
`compute_blas_budget` no longer exists. `fa5c4191` split it into
`probe_blas_heap_bytes` (raw heap measurement) and `blas_budget_for_heap`
(heap → budget), so a resize can re-derive the budget without re-probing:

```rust
// crates/renderer/src/vulkan/acceleration/predicates.rs:742
pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize, reserved_bytes: vk::DeviceSize) -> vk::DeviceSize {
    (heap_bytes.saturating_sub(reserved_bytes) / 3).max(MIN_BLAS_BUDGET_BYTES)
}
// :758
pub(super) fn probe_blas_heap_bytes(…) -> Result<vk::DeviceSize>
```

Both skill sites are wrong, each in an extra way beyond the name:

- **`audit-fnv/SKILL.md:84`** — ``predicates.rs::compute_blas_budget` =
  `device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES``. The formula is
  also stale: #3839 added the `reserved_bytes` subtraction, so the live math is
  `(heap − reserved) / 3`, not `heap / 3`. (The row's other claim — that the
  result is cached in the `blas_budget_bytes` field of `acceleration/mod.rs` —
  is still **correct**; `mod.rs:205` carries it, alongside the new
  `blas_heap_bytes` at `:210`.)
- **`audit-performance/SKILL.md:122`** — lists `compute_blas_budget` **@707**
  under Dimension entry points. Line 707 is now inside
  `screen_scaled_reservation_bytes`, an unrelated function.

**Why the gate's symbol advisory did not catch either site** — two independent
blind spots, both verified:

1. **`audit-fnv:84` is invisible to the extractor.** It writes the symbol inside
   a longer backticked span, ``predicates.rs::compute_blas_budget``. Advisory
   pass 1 matches `` `<identifier>` `` (an exactly-one-identifier span) and pass 2
   matches `` `<SYMBOL> = `` ; a `path.rs::symbol` span matches neither, so the
   token is never even considered.

2. **`audit-performance:122` *is* extracted, then suppressed by stale mentions
   in the source it checks against.** The bare span ``compute_blas_budget``
   matches pass 1, but the existence test is `grep -qw "$sym" "$src_blob"` over
   concatenated tracked source — and the rename left three dead references
   behind:
   ```
   acceleration/mod.rs:203      /// [`compute_blas_budget`](super::predicates::compute_blas_budget)
   acceleration/constants.rs:60 /// footprint. See `compute_blas_budget`.
   tests/predicates_tests.rs:253  // configuration; `compute_blas_budget` floors at 256 MB so
   ```
   Any one of them satisfies `grep -w`, so the advisory concludes the symbol
   exists. (The six other `recompute_blas_budget` hits do **not** contribute —
   `grep -w` correctly rejects them; `AccelerationManager::recompute_blas_budget`
   at `memory.rs:459` is a real, live, differently-named method.)

Blind spot 2 is the more consequential of the two and is **self-reinforcing**:
doc rot in the code immunizes doc rot in the skills, so the two halves of the
same rename hide each other. It is a fourth entry in the family documented at
`_audit-validate.sh:236-249` — #3197's (a) SCREAMING_SNAKE_CASE and (b)
negative-assertion corpus hits, and #3052's (c) `SYMBOL = value` spans.
`acceleration/mod.rs:203` is additionally a **broken rustdoc intra-doc link**
(`super::predicates::compute_blas_budget` no longer resolves).

**Impact**
`grep compute_blas_budget` returns nothing in `crates/`, so an auditor following
either entry-point list lands nowhere and may conclude the BLAS-budget path was
deleted rather than renamed. The `audit-fnv` formula error is worse than a dead
name: it describes budget math that is quantitatively wrong post-#3839, and an
auditor could file a phantom finding against correct code.

**Related**
TD3-2026-09-05-02 (this audit, Dim 3 — the **non-skill** doc sites of the same
rename; skill sites are deliberately left here to avoid double-filing).
#3842 (OPEN, filed today — the orphaned `compute_blas_budget` **code** doc
comment). #3450 (CLOSED — prior instance of two skills pinning a renamed symbol).

**Suggested Fix**
Update both sites to the split pair and drop the `@707` anchor (line numbers
drift; the gate strips them from path checks for exactly this reason). Correct
`audit-fnv`'s formula to `(heap − reserved) / 3`. Two independent gate
hardenings follow from the analysis above, and only both together close it:
add a third extractor pass for the trailing identifier of a ``path.rs::symbol``
span (fixes 1), and restrict the corpus test to *definition* sites —
`fn <sym>` / `struct <sym>` / `const <sym>` / `let <sym>` — rather than any
whole-word hit, so a stale comment can no longer vouch for a deleted symbol
(fixes 2).

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
