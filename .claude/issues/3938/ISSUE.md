# #3938 — SCR-D1-2026-09-06-01: `Pex::call_sites()` re-scans the whole debug-info table once per function (O(F·D)) and now runs synchronously on the cell-load attach path

- **Finding ID**: SCR-D1-2026-09-06-01
- **Labels**: medium,scripting,performance,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3938

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: MEDIUM (bounded-CPU hardening gap; cannot panic, OOB, or over-allocate — hence not the domain table's HIGH)
- **Dimension**: PEX Reader & Opcode Decode
- **Untrusted-Input**: **Yes**
- **Location**: `crates/pex/src/call_sites.rs:94` (`debug_lines(pex, &object.name, &scope)` once per function, before any call instruction is found) and `:172-190` (`function_infos.iter().find(..)`, four `eq_ignore_ascii_case` per candidate); consumer seam `crates/scripting/src/translate/mod.rs:152` (`analyze_pex_compatibility(&pex)` before `decompile_catching_panics`), reached from `byroredux/src/cell_loader/references/attach.rs:698` and `byroredux/src/asset_provider/script.rs:373`
- **Status**: NEW (module postdates the 2026-08-30 report)
- **Description**: both dimensions are attacker-controlled and independently `u16`-bounded per container (`function_infos` ≤ 65 535 at 9 bytes each; functions bounded only by file size at 17 bytes each). The reader is linear in file size; this pass is quadratic, executes for every scripted REFR / quest `.pex` at cell load, on the loader's thread, with no budget and no catch.
- **Evidence** (Dim 1's scratchpad harness, release, single thread; F functions of 0 instructions, D = 65 535 debug entries matching object+state but not function name):

  | F | file bytes | `parse()` | `call_sites()` |
  |---:|---:|---:|---:|
  | 60 (vanilla-shaped, D=60) | 1 670 | 22 µs | 15 µs |
  | 4 096 | 659 557 | 6.5 ms | 0.69 s |
  | 16 384 | 868 453 | 7.5 ms | 2.83 s |
  | 65 535 | 1 704 020 | 14.2 ms | **11.63 s** |

  Orchestrator confirmed the call path (`translate/mod.rs:152` precedes the decompile closure) and the per-function linear `find` shape.
- **Impact**: CPU denial-of-service at cell load from a `.pex` that passes every reader check; a second state doubles it. Vanilla/normal mod content is unaffected (15 µs at F=D=60).
- **Disproof attempted**: not memoised; not deferred until a call opcode is seen; attach path is synchronous (no spawn/thread/rayon in `attach.rs`); Champollion's `getFunctionInfo` is also a linear `find_if` but runs offline on one file — the port moved it onto a per-script load path.
- **Related**: #1710 (same "attacker-controlled count" class, different resource); #3783
- **Suggested Fix**: build one `HashMap<(object, state, function, FunctionType), &[u16]>` from `function_infos` (O(D)) and look functions up in O(1); or at minimum defer `debug_lines` until a `Call*` opcode is seen and cache per function. Regression test: F=D=65 535 completes well under a second.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
