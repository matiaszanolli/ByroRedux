# TD4-2026-10-27-01: The #6383 closed-issue advisory fires 11 times at HEAD and all 11 are false positives

Labels: low, tech-debt, bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD4-2026-10-27-01: The #6383 closed-issue advisory fires 11 times at HEAD and all 11 are false positives

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD4-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `.claude/commands/_audit-validate.sh` - the embedded Python closed-issue advisory (~:741-781: `window_cue`, `resolved`, `adjacent`).

## Description
The advisory lists 11 hits: `audit-esm:84` #5492, `audit-exterior:90` #5520, `audit-fo3:29` #5429, `audit-fo4:91` #3638, `audit-gameplay:17` #5367, `audit-oblivion:28` #6258, `audit-performance:26` #5128 and `:81` #5447, `audit-physics:24` #5538, `audit-scripting:100` #6365, `audit-ui:46` #5967. Every one is a correct past-tense citation (e.g. "all since fixed, #5493 `136048a95` / #5492 `dae08000d`", "Fixed since 2026-10-08 - verify, don't re-file: #6098 (...), #6258 (...)"). Four causes:
1. **List items are missed**: the `resolved` look-back is `[^#]{0,80}$`, so it stops at the previous `#`; the second and later numbers in a "fixed ...: #A (...), #B (...)" list are never treated as resolved.
2. **"don't re-file" is a cue**: the house idiom "fixed - verify, don't re-file" marks a closed fix, but only "regression guard" is exempted.
3. **The 160-char window spans clauses**: it catches "stays open" about a different subject in the same sentence (`exterior:90`, `physics:24`).
4. **"closed" after the number is not read**: e.g. `gameplay:17` "#5367 (...) is closed".

## Evidence
`window_cue = re.compile(r"known[- ]open|still[- ]open|stays? open|do not re-file|don't re-file")`; `guard_idiom = regression[- ]guard` only; `resolved = ...[^#]{0,80}$` (in `_audit-validate.sh` ~:741-781). The 11 hits were enumerated by the 10-27 tech-debt run.

## Impact
A day-one advisory that is 100% noise teaches auditors to scroll past it, which is the failure `_audit-common.md` warns against. The next real stale "known open" (the class tracked in #6353, #6173, #5742, #5606, #4369) will be buried in it.

## Related
#6383 (the advisory's origin, 1f6594b18), #6353.

## Suggested Fix
Treat fixed/closed/resolved anywhere earlier in the same sentence as resolving every `#N` that follows. Drop "don't re-file" as a cue when the sentence also says fixed or closed. Evaluate "open" cues per clause (split on `;`, `.`, `-`/em-dash). Add a self-test over these 11 lines plus two true positives.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

