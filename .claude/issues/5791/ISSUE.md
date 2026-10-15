# TD7-2026-10-15-01: The Papyrus VM's `new T[n]`, `Array.Add(x, count)` and string concatenation have no size cap, so one statement can exhaust memory

Labels: high,scripting,safety,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD7-2026-10-15-01`).

- **Severity**: HIGH (promotion rule "a magic/missing bound would silently over/underflow under documented use" applied to a missing bound; see Severity note)
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants
- **Location**: `crates/scripting/src/fragment/vm.rs:2039-2042` (`Expr::New`), `:2314-2320` (`"add"` array method), `:4339-4340` and `:4395` (string `+` / `StrCat`)
- **Status**: NEW
- **Age**: `Expr::New` `e766d3147` (2026-10-13, "the Papyrus VM runs quest fragments the lowering declines"); array methods `20cbfd7b7` (2026-10-13)
- **Effort**: small
- **Description**: the VM's only budgets are `MAX_STEPS = 100_000` statements per run (`:87`, enforced in `tick()` `:1038-1044`) and `MAX_DEPTH = 16` (`:89`); both count statements, not memory. Three single-statement allocations are unbounded:
  1. `new Int[n]` evaluates `n` as an `f32`, casts with `as usize` (saturating, so `1e10` stays 10 billion) and runs `vec![Value::default_for(ty); size]`. There is no cap, and `Value` is a 32-byte-class enum (`String` variant).
  2. `arr.Add(x, count)` loops `for _ in 0..int(1, 1).max(0)` pushing `wanted.clone()` each time: `count` is an `i32`, so up to 2^31-1 pushes in one step.
  3. `s = s + s` (`binary()` / `arithmetic()` build `format!("{}{}", ..)`) doubles the string per statement; about 35 statements reach terabytes, far under `MAX_STEPS`.
- **Evidence**:
  ```rust
  Expr::New { ty, size } => {
      let size = self.eval(frame, &size.node)?.number().unwrap_or(0.0).max(0.0) as usize;
      Value::Array(Rc::new(RefCell::new(vec![Value::default_for(&ty.node); size])))
  }
  ...
  "add" => { for _ in 0..int(1, 1).max(0) { items.push(wanted.clone()); } Value::None }
  ```
  The sibling static-lowering path DOES cap: `crates/scripting/src/papyrus_provider/lower_call.rs:629-635` declines `new T[n]` unless `n` is an `IntLit` `<= MAX_SCRIPT_ARRAY_ELEMENTS` (`crates/sdk/src/script_function.rs:23`, 512), and the engine's own limit is 128 (`falloutck-uesp-wiki/(main)/Arrays (Papyrus).wiki`: "up to 128 elements. Attempting to create an array larger then that will fail"; same for `Add`/`Insert`). The ahead-of-time check only recurses into the size expression (`vm.rs:5108-5111`). `catch_unwind` exists only around translation/population (`translate/mod.rs:249`, `fragment/populate.rs:312,548,707`), not around a VM run, so an allocation failure aborts the process and a `capacity overflow` panic unwinds through a scheduler system.
- **Impact**: a .pex from any installed mod (or a corrupted one) kills the engine process with a one-liner; no vanilla script does this, but the VM is the first component that executes untrusted script bodies at all (the `MAX_STEPS` limit exists precisely because of that), and the same window shipped sandbox caps for sibling attack surface (#5718 hostcall fuel, #5719 CWS inflate ceiling).
- **Severity note**: documented-use values are all <= 128, so the HIGH rests on "silent process death from untrusted input", not on a legitimate script overflowing. If the maintainers treat mod scripts as trusted, demote to MEDIUM; the fix is the same.
- **Related**: #5718, #5719, `MAX_SCRIPT_ARRAY_ELEMENTS`, `/audit-scripting`.
- **Suggested Fix**: add a named `MAX_ARRAY_ELEMENTS` (engine 128, or reuse the SDK's 512) in `vm.rs`; make `Expr::New`, `Add` (`len + count`), `Insert` and array literals `stop("array size")` past it, and cap string results at a named length in `binary()`. One test per path beside `vm/tests.rs:4900` (`Counts = new Int[0]`).


## MEDIUM Findings

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
