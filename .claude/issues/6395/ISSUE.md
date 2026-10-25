# TD8-2026-10-25-02: `txst_textures` is a write-only map plumbed through `parse_txst_group`

Labels: low, esm-plugin, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6395
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Effort**: small · **Age**: reader deleted 65add6a2e (#6036, 2026-10-20); missed by the 10-21 report
- **Location**: `crates/plugin/src/esm/records/parse.rs:162,254` (declared, passed `&mut`, never read); `crates/plugin/src/esm/cell/support.rs:621-767` (`parse_txst_group` / `_inner` param, recursion at :655, the `txst_textures.insert(header.form_id, diffuse.clone())` at :767 under the comment "Backward-compat LTEX resolver: legacy diffuse-only map"); callers: 14 sites in `cell/tests/txst.rs` + `grup_walker.rs:665`.
- **What**: #6036 moved LTEX→TXST resolution to the merged load order (`landscape_texture_txsts: ltex.ltex_to_txst`, parse.rs:536) and deleted the only loop that read `txst_textures` (`txst_textures.get(txst_id)` -> `landscape_textures`). The map is still built per TXST (a `String` clone per record) and the doc at support.rs:621 still says "kept for the LTEX resolver downstream". `txst.rs:293` even pins that MNAM-only TXSTs are absent from "the legacy map".
- **Fix**: delete the `HashMap<u32,String>` param from `parse_txst_group(_inner)`, the insert, the local in parse.rs, and the `diffuse_only` plumbing in the 14 tests (assert on `texture_sets[...].diffuse` instead, which already carries TX00).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD8-2026-10-25-02`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
