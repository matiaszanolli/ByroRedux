# #5442: NIF-D6-2026-10-08-01: the #5092 `streaming.rs` split dropped #1171's compile-time `PartialNifImport: Send` assertion (regression of #1171)

**Labels**: low,nif-parser,nif,concurrency,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5442

**Source**: `docs/audits/AUDIT_NIF_2026-10-08.md` — `NIF-D6-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: #1171 is CLOSED; validation confirmed no `assert_send` remains anywhere under `byroredux/src/` — filed as a regression.

- **Severity**: LOW (a guard removed. `Send` is still enforced at the channel send, just far from the type.)
- **Dimension**: Allocation Hygiene / pre-parse handoff (`pre_parse_cell`)
- **Game Affected**: all (the streaming NIF pre-parse path)
- **Location**: removed from the old `byroredux/src/streaming.rs:649-658`. It is absent from `byroredux/src/streaming/{mod,pre_parse,telemetry}.rs` and from the rest of the tree. The send site is `byroredux/src/streaming/pre_parse.rs:120-122` (`cell_pre_parse_worker`, `mpsc::Sender<LoadCellPayload>`).
- **Status**: Regression of #1171 (CONC-D6-NEW-05, closed). That issue added the guard. 54d713dee (#5092) removed it during what its message calls a pure move ("Glob re-exports keep every crate::streaming::* path").
- **Description**: the sorted-line diff of the pre-split file against the three new files shows only three kinds of change:
  - visibility widenings (`pub(crate)` / `pub(super)`);
  - the `#[path]` retarget and an import reshape;
  - the `legacy_lod_quads` field, which b7987d813 added later.

  The single other change is that this item vanished with its 6-line rationale comment:

  ```rust
  const _: fn() = || {
      fn assert_send<T: Send>() {}
      assert_send::<PartialNifImport>();
  };
  ```

  Its stated purpose: if a non-`Send` field such as an `Rc<…>` lands in `NifScene` or any nested import type, the error should fire at the type's declaration rather than "at the distant channel-send call deep inside `cell_pre_parse_worker`".
- **Impact**: no soundness loss, because the `mpsc` and rayon bounds still reject a non-`Send` payload. What is lost is the diagnostic locality #1171 bought, on the type that crosses from the NIF importer to the streaming worker.
- **Related**: #1171, #5092, #830.
- **Suggested Fix**: restore the `const _` block in `streaming/pre_parse.rs`, which owns `ParsedNifResult = (String, Option<PartialNifImport>)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other compile-time guards that lived in the pre-split `streaming.rs`)
- [ ] **TESTS**: A regression test pins this specific fix
