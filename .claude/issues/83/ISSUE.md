# Issue #83: LC-15: Oblivion NIF variant detection fragile

**State:** OPEN
**Labels:** bug, nif-parser, medium
**Domain:** nif

Detection uses heuristics on user_version that may misidentify edge-case Oblivion exports
on version 20.2.0.7. Add test cases for known Oblivion NIF headers.
