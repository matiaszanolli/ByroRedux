# #4084 — ESM-2026-09-09-D7-09

the `categories()` coverage guard only scans `: HashMap<` fields, so a non-`HashMap` collection added to `EsmIndex` evades both the table and the guard

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4084 --json state`).

---

- **Severity**: LOW
- **Dimension**: ESM→ECS Handoff
- **Record / Sub-record**: —
- **Location**: `crates/plugin/src/esm/records/index.rs:1411-1417` (the field scan),
  `:1456-1476` (the assertion)
- **Status**: NEW
- **Description**: `every_index_map_is_a_category_or_a_recorded_exclusion` — the
  #2907/#2990 guard that makes the ESM-D4-01 / ESM-D7-01 "41 of 92 maps silently
  dropped" defect structurally impossible — extracts fields with
  `rest.split_once(": HashMap<")`. Any index field declared as `HashSet<…>`,
  `BTreeMap<…>`, `Vec<…>` or a newtype is invisible to it, and `categories()`'s
  macros are `HashMap`-shaped too, so such a field is merged only if someone
  hand-writes an arm in `merge_from`. That is not hypothetical: **both** of the
  index's existing non-`HashMap` collections are exactly this shape —
  `deleted_record_metadata: HashSet<u32>` and `skipped_unconsumed_groups: Vec<[u8; 4]>` —
  and both are merged only because `merge_from` happens to name them explicitly
  (`index.rs:1010-1029`, `:1041-1042`).
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/records/index.rs:1411-1417
          let fields: Vec<&str> = declaration
              .lines()
              .map(str::trim)
              .filter_map(|line| line.strip_prefix("pub "))
              .filter_map(|rest| rest.split_once(": HashMap<"))
              .map(|(field, _)| field)
              .collect();
  ```
- **Impact**: latent, and narrow — every record category today is a `HashMap`. But
  the guard's stated contract (`index.rs:1469-1475`) is that a map "has no
  `categories()` row and no recorded exclusion" is impossible, and that contract has
  a typed hole. The failure mode if it is ever hit is the original one: a collection
  that accumulates on the first plugin and is silently dropped for every later one.
- **Related**: #2907, #2990, #1773; `AUDIT_ESM_2026-08-13.md` ESM-D4-01 ≡ ESM-D7-01.
- **Suggested Fix**: widen the scan to any `pub <ident>: <Type><` and require every
  field to be either in `categories()`, in `EXCLUSIONS`, or in a new
  `MANUALLY_MERGED` list naming the `merge_from` line that handles it — which would
  document `deleted_record_metadata` and `skipped_unconsumed_groups` as the
  deliberate cases they are.

---
