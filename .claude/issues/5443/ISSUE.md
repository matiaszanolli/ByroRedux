# #5443: PEX-D3-2026-10-08-01: `.pex` opcode 36 still lowers to `Expr::Cast` after #5322 added `Expr::Is`; three docs now claim agreement / absence that the code and corpus contradict

**Labels**: low,scripting,bug,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5443

**Source**: `docs/audits/AUDIT_PAPYRUS_2026-10-08.md` — `PEX-D3-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `lower.rs` ~125 still maps `op == "is"` to `Expr::Cast`; module doc still says "The shared AST has no `is`".

- **Severity**: LOW
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Location**: `crates/pex/src/decompile/lower.rs:125-134` (the `op == "is"` → `Expr::Cast` arm);
  `lower.rs:13-26` (module doc); `crates/papyrus/src/ast.rs:259-262` (`Expr::Is` doc);
  `lower.rs:558-561` (the `an_is_opcode_lowers_to_an_object_typed_cast` doc)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: These are the stale claims, each set against the current code or the corpus:

  | Location | Claim | Actual |
  |---|---|---|
  | `ast.rs` | "The two frontends agree: the `.pex` decompiler lifts `OpCode::Is` to the same binary shape" | `lift.rs:231` does produce a Node `BinaryOp "is"`, but `lower.rs:125` turns it into `Expr::Cast`. A `.psc` `f is Actor` gives `Expr::Is`, while the same line compiled to `.pex` gives `Expr::Cast` |
  | `lower.rs` module doc | "The shared AST has no `is`" | It now does |
  | `lower.rs` module doc | "≈0 vanilla scripts contain opcode 36 (no `.psc` construct emits it)" | The census found **78** `is` instructions in **37** FO4 and Starfield scripts |
  | Pin test doc | "If the AST ever gains an `Is`-capable expression, this is the test to flip" | The trigger has fired and the test was not flipped |

  The SKILL text already notes that the two frontends "do NOT yet share one shape". The code docs say
  the opposite.
- **Evidence**: Opcode census (scratch probe, this pass): all 78 type operands are object or script
  types, never a primitive. The largest groups are actor 21, container 11, referencealias 5 and
  refcollectionalias 5. `pex_corpus_smoke` independently reports `is 78`.
- **Impact**: For object types, `x as T` is truthy exactly when `x is T`, so every vanilla occurrence
  lowers to a condition with the same truth value. I found no recognizer that keys on a Cast in
  condition position and would mis-handle one. The impact is therefore the wrong documentation, plus
  two frontends that produce different AST shapes for the same source. A future consumer that matches
  `Expr::Is` will never see one from `.pex`. LOW.
- **Related**: #5322, closed #4477 (the original departure)
- **Suggested Fix**: Lower `op == "is"` to `Expr::Is`, flip the pin test to assert `Expr::Is`, and
  update the `lower.rs` module doc so it no longer lists this as a departure. Correct the census claim.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`papyrus_provider/lower_program.rs` `expression_mentions_provider` `_ => false` wildcard (cross-audit routing note))
- [ ] **TESTS**: A regression test pins this specific fix
