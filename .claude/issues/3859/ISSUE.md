# #3859: TD1-2026-09-05-10: `storage_util_form_type_id` is a 105-arm FourCC→i32 match that should be a static table

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-10) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3859 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-10), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/sdk/src/compatibility.rs::storage_util_form_type_id` (`:2694`–`:2804`)
- **Status**: NEW
- **Description**: The only match over the 50-arm flag anywhere in the eight files audited. 105
  `b"XXXX" => <i32>` arms mapping Creation Engine record signatures to the legacy `FormType`
  numbering, plus `_ => return None`. It is data, not behaviour — the exact case the dimension's
  "want a lookup table" rule names.
- **Evidence**: `b"TES4" => 1` … `b"FSTS" => 111`, with `b"NPC_" | b"CREA" => 43` the sole
  many-to-one arm and 96/97/106/107 deliberately absent.
- **Impact**: minimal at runtime (the compiler builds a jump table either way). The cost is
  reviewability: a wrong or missing signature is invisible in a 105-arm wall, and the mapping has no
  second home in the workspace to cross-check against — `grep -rn 'b"KYWD" =>' crates byroredux`
  returns this site only, so it is **not** duplicated logic (checked; not a Dimension 2 finding).
- **Related**: the memory note *Record Type Catalog* (98 classes, `RecordType` uses FourCC) —
  if a canonical `RecordType` mapping is ever added to `crates/plugin`, this becomes a duplication
  finding; today it is the only copy.
- **Suggested Fix**: `const FORM_TYPE_IDS: &[(&[u8; 4], i32)]` beside the function plus a linear or
  binary search, and one test asserting the table is sorted and has no duplicate signature. Keeps
  the `NPC_`/`CREA` alias explicit as two rows.
- **Effort**: trivial

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
