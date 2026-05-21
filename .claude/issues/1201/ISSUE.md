# #1201 — NIF-DIM4-01: NiAlphaProperty cascade gate stale (#982 only half-landed)

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, MEDIUM)
**Severity**: medium / Labels: bug, medium, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)
**Paired**: #1202 (same root, BSEffect implicit-blend site)

## Cause

`crates/nif/src/import/material/walker.rs:494` still tests `!info.alpha_blend && !info.alpha_test` instead of `!info.alpha_property_consumed`. The `alpha_property_consumed` field was added by #982 but the consumer change never landed.

## Fix

Replace the gate with `if !info.alpha_property_consumed { … }`. Round-trip test in `material/alpha_flag_tests.rs`.

## Game / Risk

Oblivion / FO3 / FNV NiTriShape legacy-property path. LOW risk (gate is strictly stricter).
