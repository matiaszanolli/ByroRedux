# TD8-2026-09-29-02: `clippy --all-targets` debt grew from 698 to 824 sites; 2 new `too_many_arguments` allows lack a reason

**Labels**: low,esm-plugin,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW (lower-priority bucket, outside the CI gate) · **Dimension**: 8 · **Status**: NEW
- **Effort**: small (plugin, mechanical) / medium (the rest)
- **Location**: workspace test and example targets; `byroredux/src/cell_loader/terrain.rs:282`,
  `byroredux/src/helpers.rs:94`
- **Evidence**:
  - `cargo clippy --workspace --all-targets --keep-going -- -D warnings` on rustc 1.96 reports 824 unique
    sites (806 errors in 39 targets). The memory note *Clippy --keep-going* measured 698 on 09-16.
  - By crate:
    - plugin 654: 522 `needless_borrows_for_generic_args`, 79 `unnecessary_to_owned`, 46
      `field_reassign_with_default`
    - byroredux/src 90
    - renderer 22
    - nif 18
    - scripting 11
    - ui 9
  - Real signal inside the bucket:
    - An unused `bridge` at `crates/ui/tests/fallout4_hudmenu_protocol.rs:36`.
    - `empty line after doc comment` at `context/geometry_pass.rs:787`, `papyrus/src/parser/script.rs:849`,
      `commands/view.rs:233` and `nif/tests/common/mod.rs:326`. This is the doc-splice class that
      UI-D5-2026-09-29-01 and PHYS-D2-2026-09-29-03 describe.
  - The two allows listed under Location have no reason comment; the other 11 added in the window do.
- **Suggested Fix**:
  - Run `cargo clippy --fix --all-targets -p byroredux-plugin`.
  - Fix the doc-splice lints by hand.
  - Add the two reason comments.
  - Consider a non-blocking `--all-targets` count lane.

**Validated at HEAD 9fcfdc3fc**: `#[allow(clippy::too_many_arguments)]` without a reason comment at `byroredux/src/cell_loader/terrain.rs:282` and `byroredux/src/helpers.rs:94`; the 824-site count comes from the audit's `clippy --all-targets --keep-going` log (`/tmp/audit/tech-debt/clippy_all.log`), not re-run here (publish rules forbid builds).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
