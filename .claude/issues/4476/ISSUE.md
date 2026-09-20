# PEX-D2-2026-09-19-01: backward_jmpt_builds_a_loop_edge tests a JmpF, not a JmpT (guard misnomer)

- **ID**: D2-01
- **Labels**: low,scripting,bug,test-gap
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4476

**Severity**: LOW (test-naming rot; no coverage hole) · **Dimension**: Decompiler CFG & Lift · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D2-2026-09-19-01) · **Location**: `crates/pex/src/decompile/cfg.rs:371-401` (misnomer); JmpT polarity actually pinned at `cfg.rs:474-475` in the differently-named #2122 sibling

**Description**
The guard a reader would credit for JmpT loop polarity, `backward_jmpt_builds_a_loop_edge`, builds its conditional from `OpCode::JmpF` (`cfg.rs:379`) with the backedge carried by the unconditional `Jmp` at `:381` — its edge assertions re-pin JmpF polarity a second time. JmpT polarity *is* pinned, but in the differently-named #2122 sibling `backward_jmpt_target_inside_own_block_conditions_the_right_block` (`on_true == 1` = jump target, `on_false == 3` = fall-through). No coverage hole; a naming/auditability defect that would misdirect a future regression hunt.

**Evidence**
`cfg.rs:379` uses `(OpCode::JmpF, vec![id("t"), Value::Integer(3)])` inside the `jmpt`-named test; contrast `cfg.rs:459` which uses `OpCode::JmpT` in the sibling.

**Impact**
An auditor or future edit greps the guard by name, concludes JmpT loop polarity is pinned here, and deletes/mutates the wrong test while breaking the real pin.

**Related**: #2122

**Suggested Fix**
Rename the test (e.g. `forward_jmpf_with_unconditional_backedge_builds_a_loop`) or add the missing mirror: a `JmpT`-conditional loop head asserting `on_true` = backedge target, `on_false` = exit.

## Completeness Checks
- [ ] **TESTS**: If renamed, the audit trail references the new name; if mirrored, both edges asserted
