**Severity**: LOW
**Dimension**: NIF Format Readiness
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` Dim 2 FIND-5

`NifVariant::detect` has a tagged-but-unresolved fork at the `(V20_0_0_4, user_version=11, _)` boundary in `crates/nif/src/version.rs:294-308`. nif.xml line 196 lists v20.0.0.4 as "Oblivion, Fallout 3" — genuinely ambiguous — and nif.xml's `#FO3#` verset (line 44) explicitly includes `V20_0_0_4__11`. The current code returns `Oblivion` (line 307); the comment at lines 297-306 acknowledges this is pinned by a test (`detect_oblivion_edge_cases`, line 563) rather than by sample data.

### Impact
Bounded — no retail FO3 NIF ships at v20.0.0.4 (retail FO3 is `(V20_2_0_7, user=11, uv2=34)`). Only bites pre-release / mod content.

### Suggested Fix
Either (a) settle the routing with a sample-data sweep against any FO3 mod corpus that ships v20.0.0.4 NIFs, or (b) downgrade to `Unknown` with a one-shot `log::warn!("ambiguous v20.0.0.4/u11 — routed as Oblivion; please file with sample")`.

### Completeness Checks
- [ ] **TESTS**: Existing test at `version.rs:563` pins current routing — update to match the resolved choice.
