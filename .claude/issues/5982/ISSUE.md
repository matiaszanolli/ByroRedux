# SCR-D2-2026-10-17-02: Float `%` casts both operands to `i32` and panics on `i32::MIN % -1`

Issue: #5982
Labels: high, scripting, bug, safety

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-02: Float `%` casts both operands to `i32` and panics on `i32::MIN % -1`

- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/vm/value.rs:367-373` (`arithmetic`, float branch of `BinaryOp::Mod`); reached by `AssignOp::ModEq` at `interp.rs:474` too
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: When either operand is not an exact `Int` the modulo falls to `if b as i32 == 0 { stop } Value::Int((a as i32) % (b as i32))`. The zero case is guarded, the overflow case is not: `f32` casts saturate, so `-2147483648.0` (or anything below) becomes `i32::MIN`, and a divisor of `-1`/`-1.5` becomes `-1`; `i32::MIN % -1` panics ("attempt to calculate the remainder with overflow") in every profile, release included - Rust emits that check unconditionally. The Int/Int branch two lines above correctly uses `wrapping_rem`.
- **Evidence**: `BinaryOp::Mod => { if b as i32 == 0 { return stop("modulo by zero"); } Value::Int((a as i32) % (b as i32)) }`. Papyrus `%` is Int-only, but nothing in the checker or the decompiler's AST types forces that, so a `.pex` `IMOD` over a Float local/property (e.g. `-2147483648.0 % -1`) reaches this arm. No `catch_unwind` around the run (see -01).
- **Impact**: One expression in a mod script aborts the process.
- **Related**: SCR-D2-2026-10-17-01.
- **Suggested Fix**: Use `(a as i32).wrapping_rem(b as i32)` (and reuse the zero stop), or stop on a non-Int operand as Papyrus does.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
