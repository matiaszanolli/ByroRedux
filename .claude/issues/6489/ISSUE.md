# PHYS-D5-2026-10-27-02: `waves_require_contact_rescan` keys on the ripple amplitude only; a swell-only wind-driven plane never re-arms the scan

Labels: low, physics, water, bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-27.md

# PHYS-D5-2026-10-27-02: `waves_require_contact_rescan` keys on the ripple amplitude only; a swell-only wind-driven plane never re-arms the scan

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-27.md` finding `PHYS-D5-2026-10-27-02` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/physics/src/water.rs` - `waves_require_contact_rescan` (~:754-772), its use (~:900) and the quiesced fast path (~:922-935); `crates/core/src/ecs/components/water.rs` `WaterMaterial::swell` (~:764-784).

## Description
The gate takes `max(material.wave_amplitude)` over the surfaces and returns `false` below 1e-4; the same value sizes the waterline hysteresis band. Since 7dd294ee3 a `wind_driven` plane also carries a travelling swell (1.5 BU calm up to 28 BU gale), and `authored_wave_height_over_bed` adds the swell whatever the ripple amplitude ("a plane with none still rolls in the wind"). The gate does not count it. When every surface in range has zero authored ripple, the quiesced fast path (`active_island_counts().0 == 0 && !pending_wake && !had_newcomers && !waves_require_rescan`) skips the scan, and the #5561 settled-depth re-wake lives inside that scan, so a sleeping float is never woken by the swell. A float can fall asleep on a calm swell: surface heave ~ A*omega ~ 1.5 * 2.08 ~ 3 BU/s vs rapier's linear sleep threshold of 0.4 m/s ~ 28 BU/s, and the `w2-dynamic-water.sh` contract allows that. Separately, when a ripple is authored but smaller than the swell, the band `max_amplitude + WATERLINE_HYSTERESIS` is too narrow for contacts the swell reaches.

## Evidence
`water.rs:755-761` reads only `surface.material.wave_amplitude`; `WaterMaterial::swell` depends only on `wind_driven` and the weather scroll; `wave_amplitude` is copied straight from WATR (`env_translate/water.rs`). The only gate test, `default_authored_waves_only_rescan_nearby_live_contacts`, uses the non-zero default ripple and no swell. The size of the affected WATR set (zero wave amplitude on Calm or still-River kinds) was not measured.

## Impact
A float on a calm lake whose WATR authors no ripple stays parked through a wind change. Content-dependent.

## Related
#3122 / #3135 (why the gate exists), #5561 (settled-depth re-wake).

## Suggested Fix
Fold the swell's current amplitude (`material.swell(weather_scroll).map(|s| s.amplitude)`, capped by `WATER_SWELL_AMPLITUDE_MAX * gain`) into `max_amplitude`; pass the weather scroll in. Add a test with `wave_amplitude: 0.0, wind_driven: true` and a sleeping submerged contact.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

