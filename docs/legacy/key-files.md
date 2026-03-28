# Key Gamebryo 2.3 Source Files

Reference paths relative to:
`/media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/`

## Scene Graph Core
- `CoreLibs/NiMain/NiObject.h` — Root base class
- `CoreLibs/NiMain/NiAVObject.h` — Transforms, properties, bounds
- `CoreLibs/NiMain/NiNode.h` — Hierarchical container
- `CoreLibs/NiMain/NiGeometry.h` — Renderable mesh base
- `CoreLibs/NiMain/NiTriShape.h` — Triangle mesh

## Streaming / NIF Format
- `CoreLibs/NiMain/NiStream.h` — File I/O, 3-phase load/save
- `CoreLibs/NiSystem/NiBinaryStream.h` — Low-level binary I/O
- `CoreLibs/NiMain/NiStreamMacros.h` — Serialization macros

## RTTI System
- `CoreLibs/NiMain/NiRTTI.h` — Runtime type info
- `CoreLibs/NiMain/NiObject.h` — NiDeclareRTTI usage

## Properties
- `CoreLibs/NiMain/NiProperty.h` — Base property + Type enum
- `CoreLibs/NiMain/NiTexturingProperty.h` — Texture maps
- `CoreLibs/NiMain/NiMaterialProperty.h` — Material colors
- `CoreLibs/NiMain/NiAlphaProperty.h` — Transparency

## Animation
- `CoreLibs/NiAnimation/NiTimeController.h` — Controller base
- `CoreLibs/NiAnimation/NiInterpolator.h` — Value interpolation
- `CoreLibs/NiAnimation/NiControllerManager.h` — Sequence state machine
- `CoreLibs/NiAnimation/NiControllerSequence.h` — Animation clip
- `CoreLibs/NiAnimation/NiKFMTool.h` — KFM file management
- `CoreLibs/NiAnimation/NiAnimationKey.h` — Keyframe types

## Rendering
- `CoreLibs/NiMain/NiRenderer.h` — Abstract renderer
- `CoreLibs/NiMain/NiMaterial.h` — Material base
- `CoreLibs/NiMain/NiStandardMaterial.h` — Built-in material
- `CoreLibs/NiDX9Renderer/` — DX9 backend (76 files)
- `CoreLibs/NiD3D10Renderer/` — D3D10 backend (45 files)

## Collision
- `CoreLibs/NiCollision/NiPick.h` — Raycasting
- `CoreLibs/NiCollision/NiOBBox.h` — Oriented bounding boxes

## System
- `CoreLibs/NiSystem/NiMemManager.h` — Memory allocation
- `CoreLibs/NiSystem/NiThread.h` — Threading
- `CoreLibs/NiSystem/NiCriticalSection.h` — Synchronization
- `CoreLibs/NiMain/NiFixedString.h` — Interned strings

## Parallelism
- `CoreLibs/NiFloodgate/NiSPTask.h` — Task unit
- `CoreLibs/NiFloodgate/NiSPKernel.h` — Compute kernel
- `CoreLibs/NiFloodgate/NiSPWorkflow.h` — Task graph

## App Frameworks
- `AppFrameworks/NiApplication/` — Game loop
- `AppFrameworks/UtilityLibs/NiEntity/` — Entity system
- `AppFrameworks/UtilityLibs/NiInput/` — Input handling
