# SCR-D1-01: Var-arg count pre-allocates up to i32::MAX elements before EOF

Filed as: matiaszanolli/ByroRedux#1710
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: HIGH
- **Dimension**: PEX Reader & Opcode Decode · Untrusted-Input: Yes
- **Location**: `crates/pex/src/reader.rs:474-481` (`read_instructions`)
- **Labels**: high, legacy-compat, bug

## Description
The var-arg path accepts `Value::Integer(n) if n >= 0` then `Vec::with_capacity(n as usize)`. `n` is attacker-controlled up to `i32::MAX` (~2.1B). `Value` carries a `String` (≥24 B), so `with_capacity(2^31)` requests tens of GB and aborts (OOM) before per-element `self.value()?` reads can hit `take`'s EOF guard.

## Impact
A ~30-byte hostile `.pex` in a modded `--scripts-bsa` aborts the process at cell load. Untrusted-input DoS.

## Suggested Fix
Use `Vec::new()` + push (geometric growth EOFs at first read past buffer), or cap with a sane ceiling.
