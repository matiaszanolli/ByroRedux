# 3263: CONC-D3-2026-08-24-05: weather_system's WeatherDataRes->WeatherTransitionRes hold order is undocumented

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D3-2026-08-24-05)

## Description

`weather_system` holds a `WeatherDataRes` read guard for 220 lines and acquires `WeatherTransitionRes` reads three times inside that span, establishing `WeatherDataRes → WeatherTransitionRes`. Its sibling `promote_weather_transition_target` walks the pair in reverse: `WeatherTransitionRes` read, seven field copies, `drop(tr)` at `:829` (no comment), then `WeatherDataRes` write. That uncommented `drop` is the single line preventing a two-node cycle.

## Location

`byroredux/src/systems/weather.rs:466-685` (hold span), `:544-546`, `:664-670`, `:675-680`, `:819-830` (reverse-order sibling)

## Impact

Deleting/moving `weather.rs:829` creates `WeatherTransitionRes → WeatherDataRes` against the existing reverse edge, a length-2 cycle. Under `BYRO_LOCK_ORDER_CHECK=1` that aborts; without it, a real hang the moment `weather_system` leaves the exclusive lane.

## Related

#2269, #2153, #2154, #3111, #1103.

## Suggested Fix

Add a two-line lock-order note at `weather.rs:466` and at the `drop(tr)` naming the invariant. More robust: a `try_resource_2_mut`-style paired accessor.

## Completeness Checks
- [ ] **LOCK_ORDER**: Invariant documented at both sites, or enforced structurally
