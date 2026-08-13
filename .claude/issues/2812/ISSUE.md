# REN-D18-04: No-climate TOD fallback quad [6,10,18,22] duplicated across three sites

Labels: low, renderer, bug

## Description

The no-climate TOD quad `[6, 10, 18, 22]` is written out independently in three places, consumed by different producers of the *same* "exterior, no authored climate" state — violating the rule `apply_neutral_exterior_fallback`'s own doc records ("the **one** canonical EXAL boundary fallback … not a private set", the #1722 lesson). `DEFAULT_TOD_HOURS`'s doc even *asserts* the coupling while referencing neither of the other two literals. A future re-anchor applied to one or two silently splits the fallback sun arc from the fallback palette interpolation.

## Location

`byroredux/src/env_translate.rs` (`climate_tod_hours`'s `FALLBACK`, `procedural_fallback_weather`), `byroredux/src/systems/weather.rs` (`DEFAULT_TOD_HOURS`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D18-04).

https://github.com/matiaszanolli/ByroRedux/issues/2812
