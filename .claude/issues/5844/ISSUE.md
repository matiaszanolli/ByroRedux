# TD7-2026-10-15-02: Three different bare literals bound "follow the keyword's linked-ref chain" in the same VM, and `GetNthLinkedRef` silently truncates at 64

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD7-2026-10-15-02`).

- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants
- **Location**: `crates/scripting/src/fragment/vm.rs:2889` (`seen.len() > 256`, Enable/DisableLinkChain), `:3241` (`map_or(100, ..)`, GetLinkedRefChain default, with a caller-supplied `limit` that is otherwise unbounded), `:3721` (`.min(64)`, GetNthLinkedRef)
- **Status**: NEW
- **Age**: `12fb0c549`, `7caefeeb1`, `d7b7f5f78` (2026-10-13)
- **Effort**: trivial
- **Description**: the `100` is the CK-documented `iMaxExpectedLinkedRefs` default (the comment at `:3232-3234` says so); the `256` and `64` have no source and no name. A script calling `GetNthLinkedRef(70)` gets the 64th link, not the 70th or `None`; `EnableLinkChain` stops at 256 links while `GetLinkedRefChain` stops at 100 over the same graph.
- **Evidence**: `for _ in 0..(hops.max(0.0) as usize).min(64)`; `if !seen.insert(link) || seen.len() > 256 { break; }`.
- **Impact**: wrong ref for deep chains (no vanilla chain is that long), and three unrelated tunables for one concept.
- **Related**: TD7-2026-10-15-01 (same file, same missing-ceiling class).
- **Suggested Fix**: one `MAX_LINK_CHAIN_HOPS` const (CK default 100 as the documented value), clamp the caller's `limit` to it, and make `GetNthLinkedRef` return `None` past the cap instead of the capped hop.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
