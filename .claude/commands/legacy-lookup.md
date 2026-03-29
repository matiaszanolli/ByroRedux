---
description: "Look up a Gamebryo class, function, or pattern in the legacy 2.3 source"
argument-hint: "<class-name or search term>"
---

# Legacy Gamebryo Lookup

Search the Gamebryo 2.3 source for the given class, function, or pattern.

## Source Location
`/media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/`

## Process

1. **Search headers first** (most documentation is in .h files):
   ```
   Grep for "$ARGUMENTS" in:
   - CoreLibs/NiMain/
   - CoreLibs/NiAnimation/
   - CoreLibs/NiCollision/
   - CoreLibs/NiSystem/
   - SDK/Win32/Include/
   ```

2. **Read the matching header** — report the full class declaration with all public/protected members.

3. **Check for .cpp implementation** if the header has method signatures without bodies.

4. **Cross-reference with Redux** — check if we already have an equivalent in `crates/core/src/ecs/components/` or `docs/legacy/`.

5. **Report**:
   - Full class hierarchy (what it inherits from)
   - Key methods and their signatures
   - How it maps to Redux (existing component, or what would be needed)
   - Any streaming/serialization macros (NiDeclareStream, NiDeclareRTTI)

## Examples
```
/legacy-lookup NiTriShape
/legacy-lookup NiAlphaProperty
/legacy-lookup NiControllerSequence
/legacy-lookup NiBSplineInterpolator
```
