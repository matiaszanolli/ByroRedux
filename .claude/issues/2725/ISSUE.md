# #2725: Six multinames, nine strings and two namespaces in the adapter ABC are dead and shipped into every patched menu

- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/ui/src/avm2_host.rs:516-563`, `:857-862`
- **Status**: NEW
- **Description**: The constant pool declares entries no emitted opcode ever
  references. Multiname slots **2** (`flash.display::LoaderInfo`), **6**
  (`getLoaderInfoByDefinition`), **7** (`addEventListener`), **8** (`target`),
  **9** (`content`) and **15** (`flash.utils::setTimeout`) are never bound to a
  local and never appear in any `Op`. The strings backing them — plus
  `"complete"` (1-based 13), which no `qname` even references — and the
  `flash.display` / `flash.utils` namespaces are dead with them.
- **Evidence**: The `let` bindings at `:564-580` cover multiname positions
  1, 3, 4, 5, 10, 11, 12, 13, 14, 16, 17 (plus `root_slot`, appended
  dynamically). Positions 2, 6, 7, 8, 9, 15 have no binding and no literal use
  anywhere in the function. The module's own history explains why: the doc at
  `docs/engine/ui.md` states the adapter patches the lifecycle constructor
  specifically to **avoid** "Ruffle's intentionally stubbed
  `LoaderInfo.getLoaderInfoByDefinition` root lookup" — i.e. the
  `LoaderInfo`/`addEventListener`/`target`/`content`/`complete` chain is the
  *superseded* strategy's vocabulary, and the `setTimeout` chain is a deferral
  mechanism that also went unused.
- **Impact**: Small and bounded — ~150 bytes of dead constant pool written into
  every FO4 SWF the engine patches, and ~15 lines of misleading declaration
  suggesting a load-event path the adapter does not take. Zero runtime cost
  beyond parse.
- **Related**: TD7-2026-08-12-01 is why this has not already been cleaned:
  deleting any of these entries renumbers everything after it, so the cleanup
  is gated on the index refactor. Do them in one commit.
- **Suggested Fix**: Delete the six multinames, the nine strings and the two
  namespaces **after** TD7-2026-08-12-01 lands. Verify with
  `generated_adapter_is_valid_abc_with_one_helper_per_method` plus one
  `--ignored` run of `installed_fallout4_host_calls_are_cataloged`.
- **Effort**: trivial once TD7-2026-08-12-01 is done; do not attempt before.

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD8-2026-08-12-01`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

