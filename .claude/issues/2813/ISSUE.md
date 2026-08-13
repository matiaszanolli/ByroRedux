# REN-D18-05: Peak sun intensity 4.0 duplicated across producer, bootstrap seed, and divisor

Labels: low, renderer, bug

## Description

Peak sun intensity `4.0` is spelled independently in the producer, the bootstrap seed, and the divisor it is normalised by — three unrelated declarations in different modules that must be equal for the exterior directional ramp to span `[0, 1]`. Both sun-arc tests assert against their own hardcoded `4.0`/`3.6`, so they stay green through a one-sided change. Raising the producer alone saturates the ramp early; lowering it alone caps daytime exterior directional below full strength. Whole-frame exterior lighting, silently.

## Location

`byroredux/src/systems/weather.rs` (`compute_sun_arc`), `byroredux/src/env_translate.rs` (`SUN_INTENSITY`), `byroredux/src/render/mod.rs` (`SUN_INTENSITY_PEAK`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D18-05).

https://github.com/matiaszanolli/ByroRedux/issues/2813
