# #2728: The Tag::DoAbc/DoAbc2 payload match is written out four times plus two unreachable!() arms

- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/ui/src/avm2_host.rs:58-61`, `:77-81`, `:108-112`, `:209-213`
- **Status**: NEW
- **Description**: Four copies of

  ```rust
  let data = match tag {
      Tag::DoAbc(data) => Some(*data),
      Tag::DoAbc2(do_abc) => Some(do_abc.data),
      _ => None,
  };
  ```

  The `:108` copy is the non-`Option` variant and carries
  `_ => unreachable!("root ABC index must reference an ABC tag")`; `:124` has a
  second `unreachable!()` for the same discriminant in the replacement-tag
  match. CLAUDE.md's global rule is explicit that logic is improved in place,
  not duplicated.
- **Impact**: Low but real: any future SWF tag that can carry ABC (or a change
  in the pinned `swf` crate's tag enum) must be added in four places, and two
  of them panic rather than degrade if missed.
- **Suggested Fix**: One private `fn abc_payload<'a>(tag: &Tag<'a>) -> Option<&'a [u8]>`;
  the three `Option` sites become `abc_payload(tag)`, and the `:108` site becomes
  `abc_payload(&movie.tags[root_abc_index]).ok_or_else(…)?`, converting one of
  the two `unreachable!()` panics into an ordinary `Result` — consistent with
  the rest of the module, which is `Result`-based throughout.
- **Effort**: trivial

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD2-2026-08-12-01`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

