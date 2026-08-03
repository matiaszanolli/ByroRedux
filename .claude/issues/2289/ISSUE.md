# SCR-D5-NEW5-02: Several new effect primitives ship with zero decline-path test coverage

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2289
**Source audit**: `docs/audits/AUDIT_SCRIPTING_2026-08-03.md`
**Severity**: LOW (test-coverage gap; every primitive checked is structurally sound by inspection)
**Dimension**: Recognizer-Chain Soundness (Dimension 5)
**Location**: `crates/scripting/src/translate/effects.rs` test module (lines 959-1527)
**Labels**: low, tech-debt, bug

## Body

(see GitHub issue for full body — description, evidence, impact, suggested fix, completeness checklist)

Summary: of ~26 new effect primitives added this session, roughly half lack a decline-path test pinning their `?`/arg-count/arg-type guard (`SetOpen`, `SetPlayerRestrained`, `SetPlayerControls` family, `SetPlayerAiDriven`, `SetHudCartMode`, `PlayIdle`, `SetVehicle`, `TetherToHorse`, `SetMotionType`'s own decline path, `SetSittingRotation`, `ExitCart`, `PlayerImodAnimation`/`PlayerFurnitureAnimation`, `EvaluatePackage`, `Wait`, `StartScene`/`StopScene`). No defect today; a future refactor could silently loosen a guard undetected.
