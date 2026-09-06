# #3889: TD8-2026-09-05-06: `MaterialProvider::register_starfield_cdb` is a test-only duplicate of the shipped CDB registration path, and its doc names a production caller that calls a different method

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-06) via `/audit-publish`, 2026-09-05. Labels: `low,import-pipeline,game:starfield,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3889 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-06), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/asset_provider/material.rs` (`register_starfield_cdb`, allow at line 598; `discover_starfield_cdbs`; `register_starfield_cdb_probe`)
- **Status**: NEW
- **Effort**: trivial (≤30 min) to small (≤2 h, if the `peek_magic` reject is restored to the live path)

**Description**
`register_starfield_cdb(&mut self, bytes: &[u8])` opens with:
```rust
/// Validate + register a Starfield `materialsbeta.cdb` payload for the
/// presence gate — `discover_starfield_cdbs` calls this once per CDB
/// found across the loaded archives (#1571).
```
`discover_starfield_cdbs` does not call it. It extracts the payload, calls `ComponentDatabaseFile::probe_header` itself, and then calls the *private* `register_starfield_cdb_probe(info)` — the one-line `self.sf_cdb_count += 1` sibling. `register_starfield_cdb` is reached only from `byroredux/src/asset_provider/tests/starfield_mat.rs` (8 call sites).

Two consequences follow:

1. **The eight `starfield_mat.rs` tests exercise a parallel copy of the registration path, not the shipped one.** They validate `peek_magic` rejection, `probe_header` failure logging and the count increment through a function production never executes.
2. **The `peek_magic` cheap-reject added by SF-D3-AUDIT-03 / #2102 exists only in the dead copy.** `discover_starfield_cdbs` goes straight to `probe_header`. This is a lost micro-optimisation rather than a correctness gap — `probe_header` → `Parser::parse_header` validates the `BETH` signature anyway — but the two paths also emit *different* diagnostics for the same malformed input, so a real-world CDB rejection logs a different message than every test asserts against.

**Evidence**
```
$ sed -n '177,225p' byroredux/src/asset_provider/material.rs   # discover_starfield_cdbs
        …
        let probe = ComponentDatabaseFile::probe_header(&raw).ok();   # no peek_magic
        …
        if let Some(info) = probe { provider.register_starfield_cdb_probe(info); }

$ grep -RIn "register_starfield_cdb\b" --include="*.rs" byroredux crates
  byroredux/src/asset_provider/material.rs:599                    # definition
  byroredux/src/asset_provider/tests/starfield_mat.rs: 81,82,92,107,170,314,330,358   # 8 test calls
  →  no production caller
```

**Impact**
The test suite's coverage of Starfield CDB presence detection is a fiction: it can stay green while `discover_starfield_cdbs` regresses. Low blast radius today (the gate is presence-only, Phase 1), but Phase 2's per-field CDB index will be built on top of this path.

**Related**: #1571, #2100 (SF-D3-AUDIT-01, `probe_header`), #2102 (SF-D3-AUDIT-03, `peek_magic`), memory note "Starfield CDB Phase 2 Unblocked"

**Suggested Fix**
Delete `register_starfield_cdb` and repoint the eight tests at `discover_starfield_cdbs` (which already has an in-memory BA2 fixture builder — `starfield_mat.rs:258` references one). If the `peek_magic` fast reject is worth keeping, move it into `discover_starfield_cdbs` before the `probe_header` call rather than leaving it stranded in a dead function.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
