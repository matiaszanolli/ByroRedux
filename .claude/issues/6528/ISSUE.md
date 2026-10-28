# TOOL-D3-2026-10-27-01: `InputAction::parse` has no "vats" arm, so a manifest that filters input on the VATS action is refused

Labels: medium, tech-debt, test-gap, game:fnv, game:fo3, bug
Source: docs/audits/AUDIT_TOOLING_2026-10-27.md

# TOOL-D3-2026-10-27-01: `InputAction::parse` has no "vats" arm, so a manifest that filters input on the VATS action is refused

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-27.md` finding `TOOL-D3-2026-10-27-01` (severity MEDIUM, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/sdk/src/event.rs` - `InputAction::parse` (~:711-727; `as_str` at ~:691-707). Consumers: `crates/sdk/src/manifest.rs` (validation, ~:405-406) and `byroredux/src/extensions/install.rs:187` (`filter_map(SdkInputAction::parse)`). Added by 818eba7f4 (Refs #6350).

## Description
818eba7f4 added `InputAction::Vats` to the SDK enum, to `as_str` (`"vats"`), to the WIT `input-action` enum (`crates/mod-runtime/wit/host.wit:93`) and to the host mapping (`byroredux/src/extensions/mod.rs:464`). It did not add `"vats" => Self::Vats` to `parse`, the only path from a manifest filter's `equals` string to an action. `Manifest::validate` therefore turns `equals = "vats"` into `ManifestError::InvalidInputAction("vats")`: the action's own `as_str` spelling fails to parse.

## Evidence
`parse` lists 13 arms (`move-forward` ... `quickload`, `pause`) while `as_str` has 14 (`Self::Vats => "vats"`). `serde(rename_all = "kebab-case")` derives `"vats"` for the serde path, so only the hand-written parser is out of step. No test round-trips every variant through `as_str` -> `parse`; the only negative-case test is `raw-key-f13` (`manifest.rs` ~:832-835).

## Impact
An extension that wants VATS-key events only cannot declare that: its manifest is rejected outright and every other subscription in it goes too. Workaround: an unfiltered `input-action` subscription filtered inside the guest (`dispatch.rs:366` treats an empty filter set as "all actions"). The next action added to the enum will drift the same way.

## Related
#6350 (VATS runtime tracker), 818eba7f4.

## Suggested Fix
Add the `"vats"` arm. Then write a test walking every variant through `parse(as_str(v)) == Some(v)`, via a const `ALL` slice or an exhaustive `match` in the test so a new variant fails to compile. Do the same for `SessionPhase`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

