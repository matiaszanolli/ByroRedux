# #3757: FO3-2026-08-30-D1-01: two texture_registry doc comments invert the clamp-mode default — "clamped to 0 (REPEAT)" is wrong in both halves

**Labels**: documentation, renderer, low, legacy-compat, game:fo3, doc-rot
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_FO3_2026-08-30.md` · **Severity**: LOW · **Dimension**: 1 (inline-shader material path — documentation)
**Game affected**: shared across games; surfaced on FO3

## Location
- `crates/renderer/src/texture_registry.rs` — `load_dds_with_clamp`'s doc (currently `:611-612`) and `get_by_path_with_clamp`'s doc (`:1140-1141`)

## Description
`load_dds_with_clamp`'s doc says *"`clamp_mode` values outside `0..=3` are clamped to `0` (REPEAT) — defensive default for upstream parser garbage"*, and `get_by_path_with_clamp`'s says *"Defaults (`clamp_mode == 0` = REPEAT) preserve the legacy single-key shape"*.

**Both are wrong twice over** — wrong about which index is REPEAT, and wrong about which direction the clamp goes.

## Evidence
Re-verified against current source 2026-08-30:
- `SAMPLER_ADDRESS_MODES` index **0** is `(CLAMP_TO_EDGE, CLAMP_TO_EDGE)`; index **3** is `(REPEAT, REPEAT)`.
- The code clamps **up**, not to zero: `let clamp_mode = clamp_mode.min(3);` at both sites.
- The regression test is named `out_of_range_clamp_mode_falls_back_to_3` (`crates/renderer/src/texture_registry_tests.rs`).
- `get_by_path` passes `3` with the comment *"3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT cache entry"*, contradicting the doc four lines below it.

## Impact
No runtime defect — **the code is right, the prose is wrong**. It surfaced now because #3516 made FO3 the first title whose legacy-chain clamp values are non-trivial (post-#3516 the FO3 `NiTexturingProperty` clamp histogram moved from `{0: 2077}` to `{0: 21, 2: 2, 3: 2054}`), so this is the doc the next reader consults while reasoning about FO3/FNV sampler addressing. Shared across games.

## Related
#3516 (the TexDesc clamp-nibble fix that made FO3 clamp values non-trivial), #3517 (the `NiTexturingProperty` clamp-writer precedence issue — a different defect on the same subject, still open), #208.

## Suggested Fix
Replace both docs with: *"out-of-range `clamp_mode` is clamped to `3` (`WRAP_S_WRAP_T` = REPEAT/REPEAT); mode `0` is `CLAMP_S_CLAMP_T`."*

## Completeness Checks
- [ ] **SIBLING**: check every other `clamp_mode` doc/comment in the registry and its callers for the same inversion
- [ ] **TESTS**: `out_of_range_clamp_mode_falls_back_to_3` already pins the behaviour — no new test needed, but the doc must agree with it
