# Shared Audit Protocol — Gamebyro Redux

## Project Layout

```
Core ECS:    crates/core/src/ecs/
Components:  crates/core/src/ecs/components/
Resources:   crates/core/src/ecs/resources.rs
Strings:     crates/core/src/string/
Math:        crates/core/src/math.rs
Types:       crates/core/src/types.rs
Renderer:    crates/renderer/src/vulkan/
Mesh:        crates/renderer/src/mesh.rs
Vertex:      crates/renderer/src/vertex.rs
Shaders:     crates/renderer/shaders/
Platform:    crates/platform/src/
CXX Bridge:  crates/cxx-bridge/
Binary:      gamebyro-redux/src/main.rs
Legacy Ref:  docs/legacy/
```

## Legacy Source (for compatibility audits)

```
Gamebryo 2.3: /media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/
  CoreLibs/NiMain/       Scene graph, rendering, materials
  CoreLibs/NiAnimation/  Controllers, interpolators, keyframes
  CoreLibs/NiCollision/  OBB trees, raycasting
  CoreLibs/NiSystem/     Memory, threading, I/O
  SDK/Win32/Include/     1,592 public headers
```

## Context Management

- Max 1500 lines per Read — use offset/limit for larger files
- Grep before Read — find the pattern first, then read only relevant sections
- Incremental writes — append findings to report as you go
- One dimension at a time — complete before starting next

## Deduplication Protocol

Before reporting any finding:
1. Run: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels`
2. Scan `docs/audits/` for prior reports mentioning the same code location
3. Mark each finding: **NEW** | **Existing: #NNN** | **Regression of #NNN**
4. Skip creating issues for findings that already have an OPEN issue

## Report Output

Save to: `docs/audits/AUDIT_<TYPE>_<YYYY-MM-DD>.md`

## Finding Format

```markdown
### <ID>: <Short Title>
- **Severity**: CRITICAL | HIGH | MEDIUM | LOW
- **Dimension**: <audit area>
- **Location**: `<file-path>:<line-range>`
- **Status**: NEW | Existing #NNN | Regression of #NNN
- **Description**: What is wrong and why
- **Evidence**: Code snippet
- **Impact**: What breaks
- **Suggested Fix**: Recommended approach
```

## Domain Labels

Severity: `critical`, `high`, `medium`, `low`
Domain: `ecs`, `renderer`, `vulkan`, `pipeline`, `memory`, `sync`, `platform`, `cxx`, `nif`, `animation`, `legacy-compat`, `performance`, `safety`
Type: `bug`, `enhancement`, `maintenance`
