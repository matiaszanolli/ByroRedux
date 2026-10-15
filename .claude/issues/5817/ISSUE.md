# TD2-2026-10-15-16: Three console commands parse FormIDs with an inline hex-only chain while `parse_console_u32` is decimal-first, so the same token means different things across commands

Labels: low,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-16`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/commands/gameplay.rs:57-59` (`inv.add`), `:97` (`inv.equip`), `:434-436` (`levelup perk`) vs `byroredux/src/commands/shared.rs:81-87` (`parse_console_u32`, used by 12 `quest.rs` sites, `cndf.rs`, `condition.rs`, `anim_archetype.rs`, `actor_value.rs`, `script_var.rs`)
- **Status**: NEW
- **Age**: `parse_console_u32` is the M45/M47 helper; the gameplay commands predate it
- **Effort**: trivial
- **Description**: `u32::from_str_radix(raw.trim_start_matches("0x"), 16)` (a bare `00014` is hex; `0X` is not stripped) vs `parse_console_u32` (a bare `14` is decimal; `0x`/`0X` is hex). `inv.add 10` adds form 0x10, `quest.force 10` addresses form 10 decimal.
- **Evidence**: `grep -n 'from_str_radix' byroredux/src/commands/*.rs`.
- **Impact**: console FormID entry is inconsistent; a debugging session mixing `quest.force` and `inv.add` mis-addresses forms. Not a correctness bug in shipped gameplay.
- **Related**: `crates/scripting/src/papyrus` token hex (`token.rs:88`) is a third spelling.
- **Suggested Fix**: add `parse_console_form_id(tok) -> Option<u32>` (hex with optional 0x/0X) in `shared.rs`, document that FormID arguments are hex, and route the three sites and `script_var.rs:167`'s `strip_prefix("0x").and(...)` through it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
