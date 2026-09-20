# PEX-D1-2026-09-19-02: pin the transmute invariant (MAX_OPCODE == last discriminant + 1) with a const assert

- **ID**: D1-02
- **Labels**: low,scripting,bug
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4475

**Severity**: LOW (hardening) · **Dimension**: PEX Reader · **Untrusted-Input**: Yes (the invariant protects `from_u8` on hostile bytes)
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D1-2026-09-19-02) · **Location**: `crates/pex/src/opcode.rs:68,130-137,160-166`

**Description**
The `unsafe transmute` in `from_u8` is sound only while `OpCode` has contiguous discriminants `0..MAX_OPCODE` with `MAX_OPCODE == 51 == last discriminant + 1`. Today that holds (51 variants, `TryLockGuards == 50`, guard `byte >= MAX_OPCODE`), and the SAFETY comment says so. But the only enforcement is the runtime test `discriminants_match_on_disk_order` — nothing fails at *compile time* if a variant is added or `MAX_OPCODE` is edited without re-deriving it. An appended variant with a stale `MAX_OPCODE` is unreachable (safe but wrong); a mid-enum insertion shifts every discriminant and can push `OPCODES[self as usize]` (in `name()`/`arg_count()`) out of bounds for out-of-table discriminants.

**Evidence**
`opcode.rs:134-136` SAFETY comment; `OPCODES` length tied to `MAX_OPCODE as usize` (`:73`); the pin lives in `#[cfg(test)]` only; `grep 'const _: () = assert'` finds nothing.

**Impact**
None today (CI runs the test). The failure mode if the test is skipped or deleted is a wrong-arg-count decode or an index panic on hostile bytes.

**Related**: #2127 (full-table test), #3948 (panic net)

**Suggested Fix**
One line next to `MAX_OPCODE`:
```rust
const _: () = assert!((OpCode::TryLockGuards as u8) + 1 == MAX_OPCODE,
    "MAX_OPCODE must be the last opcode discriminant + 1");
```

## Completeness Checks
- [ ] **UNSAFE**: The SAFETY comment on `from_u8` is updated to reference the const assert as the compile-time half of the invariant
- [ ] **TESTS**: `discriminants_match_on_disk_order` stays (defense in depth)
