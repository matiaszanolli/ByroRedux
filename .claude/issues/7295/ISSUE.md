# #7295: SCR-D6-03: The unknown-operator log-once list is unbounded and scanned linearly per token

- **Severity**: MEDIUM (the shared table says unbounded allocation on mod bytes is HIGH; downgraded because the list can hold at most the distinct tokens already inside the loaded SCDA, so the amplification is CPU, not memory)
- **Dimension**: Legacy ObScript
- **Untrusted-Input**: Yes (SCDA expression bytes)
- **Location**: `crates/scripting/src/obscript_vm.rs:1444-1449` (reached from `eval_expr_value`, `:1021-1033`)
- **Status**: NEW — introduced by b485627e3 (SCR-2026-11-15-01 fix)
- **Description**: the fix for the missing `%` made the unknown-operator arm log "once per token" through a process-wide `static REPORTED: Mutex<Vec<String>>`. The token is whatever space-separated byte run the expression holds (`from_utf8(..).unwrap_or("")`, then `parse::<f32>` falls through to `apply_operator`). An expression is a `u16`-framed payload (up to 65 535 bytes), so one statement can carry about 20 000 distinct garbage tokens. Each unseen token pushes onto the `Vec` and logs a `warn!`; each repeat does `reported.iter().any(..)` over the whole list under the mutex. The `Vec` is never cleared and never capped.
- **Evidence**:
  ```rust
  static REPORTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
  if let Ok(mut reported) = REPORTED.lock()
      && !reported.iter().any(|seen| seen == token) {
          reported.push(token.to_string());
          log::warn!("ObScript expression uses an operator the VM does not know: {token:?} (read as 0)");
      }
  ```
- **Impact**: a hostile or corrupt plugin ships one object-script `GameMode` block with a ~64 KB expression of distinct tokens. After the first run the list holds ~20k entries, and every later frame's run costs on the order of 20k × 10k string compares (about 10^8, tens to hundreds of ms) under a global lock, plus a warn flood on the first run. The same list grows across every script the session loads. The old arm was silent and O(1), so this is a regression in robustness on mod-supplied input, not a correctness change.
- **Suggested Fix**: cap the list (for example 32 entries, then log "further unknown operators suppressed" once) or key a `HashSet` with a length cap on the token text; better, move the report to the per-script compile/attach step (`ObScriptProgram::new`) so it is once per script, not per evaluation.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
