**Severity**: LOW (defensive gap; zero observed instances in vanilla SF masters) · **Dimension**: ESM + Cell Bring-up
**Location**: `crates/plugin/src/esm/cell/walkers.rs` (`game == Starfield && len == 108`) → fall-through Skyrim `len >= 92` arm
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D5-03)

## Description
The dedicated SF decode branch is gated on exact `== 108`. All ~11 985 vanilla SF cells ship exactly 108. A modded/future-DLC SF cell at any other size ≥ 92 would skip the SF arm and be decoded by the Skyrim ambient-cube/specular/fresnel path, misreading the height-fog bytes. `xcll_size_sanity_warn` fires, so the symptom is at least logged.

## Evidence
Exact-equality gate (`game == Starfield && len == 108`, documented at `walkers.rs:31-32`) vs the `>= 92` Skyrim gate it falls through to.

## Impact
Negligible for vanilla (no non-108 SF cell exists); only a hypothetical mod would get mis-lit fog, and the sanity-warn surfaces it.

## Related
SF-D5-02 (same SF XCLL decode path).

## Suggested Fix
Optional hardening — broaden to `game == Starfield && len >= 108` so any SF-classified cell takes the SF path regardless of trailing pad.

## Completeness Checks
- [ ] **SIBLING**: The `>= ` broadening matches how the Skyrim/FNV arms already gate (`>= 92`), so SF is consistent with siblings
- [ ] **TESTS**: A test feeds a >108 SF-classified XCLL and asserts the SF arm (not Skyrim) decodes it
