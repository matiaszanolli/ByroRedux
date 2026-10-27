# CHAR-2026-10-26-L-13: `stealth::detection_score` attenuation is not monotonic past `MaxDist`

- **Severity**: LOW
- **Dimension**: Derived Formulas (CHARAL-adjacent)
- **Game**: fo3, fnv
- **Location**: `crates/core/src/stealth.rs:243-244`; tests `attenuation_matches_the_captured_expression` sweep only `distance ≤ max_distance`
- **Status**: NEW
- **Source**: `charal-fnv-fo3-ruleset.md:260` gives `((MaxDist − d)/MaxDist)²` with no out-of-range statement. The transcription is exact. The observation follows from the expression (`d = 2·Max` gives 1.0, the same as `d = 0`).
- **Description**: `distance` is unconstrained, and the doc does not tell a future consumer to gate on `distance ≤ MaxDist`. The module has no consumers today.
- **Impact**: A latent trap for the first detection system: an unguarded far call reports "Detected".
- **Suggested Fix**: Document the precondition on `DetectionInputs::distance`. Add a floor only if the source confirms detection is range-bounded.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-13`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

