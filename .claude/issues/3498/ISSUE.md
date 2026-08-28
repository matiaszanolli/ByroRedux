# #3498: SCR-D5-2026-08-27-04: fragment_coverage's module doc promises a decline-reason tally the implementation does not produce — the instrument meant to make the next primitives obvious cannot answer that

**Labels**: low, scripting, test-gap, bug
**Filed**: 2026-08-27 (`/audit-publish` of `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`)

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/examples/fragment_coverage.rs:1-22` (the claim) vs `:155-165` (the tally loop)
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

The harness's module doc states it will *"tally claimed vs declined (with the decline reasons, so the next primitives to add are obvious)"*. The implementation tallies `behavioral`, `claimed`, `empty`, and a per-`Effect`-kind histogram of the fragments that **did** lower — and nothing at all about the ones that declined.

Since a fragment declines wholesale on its first unmodeled statement, the 8,986 (Skyrim) + 18,325 (FO4+Starfield) declined fragments are reported as a single number with no attribution. The 2026-08-27 audit could not use the harness to answer "why is `MoveTo` zero" and had to build a separate AST-walking probe to get there; its two headline findings came from that probe, not from the checked-in instrument.

## Evidence

```rust
// fragment_coverage.rs:155-165 — the entire tally
if let Some(effects) = lower_fragment_with_quest_properties(&func.body, &quest_properties) {
    claimed += 1;
    claimed_effects += effects.len();
    for e in &effects {
        *effect_hist.entry(effect_kind(e)).or_default() += 1;
    }
}
```

There is no `else` arm, and `grep -n decline crates/scripting/examples/fragment_coverage.rs` returns only the module-doc claim (`:10`) and the summary print (`:184`) — no `decline_hist` anywhere in the file.

## Impact

The domain's one empirical coverage instrument reports *what works* and is blind to *what doesn't* — which is the half that drives roadmap decisions about which primitive to write next. Directly explains why SCR-D5-2026-08-27-01 survived four prior audit passes: the harness reports a `MoveTo` structural-zero identically to "authors don't use MoveTo".

## Related

SCR-D5-2026-08-27-01 (the finding this gap concealed). A second, smaller instance of the same class in the same range: `byroredux/src/asset_provider/script.rs:74-85` still says the quest-fragment walk *"Runs once per cell load"*, three lines above the #3161 latch that made it run once per session.

## Suggested Fix

Record, per declined fragment, the first statement shape that failed to classify (method name + arity is enough — that is exactly what would have surfaced `moveto/5` and `enable/1` at the top of the list), and print the top ~30. Small change to a non-shipping example; high leverage for every future primitive decision. Fix the stale `script.rs` docstring in the same change.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`pex_corpus_shapes` / `pex_corpus_smoke` — do they report their own failure attribution?)
- [ ] **TESTS**: A regression test pins this specific fix
