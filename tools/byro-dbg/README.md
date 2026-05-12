# `byro-dbg`

External TCP debugger for a running ByroRedux engine. Connects over
length-prefixed JSON, issues Papyrus-style expressions + dotted
console commands, prints the results.

## Quick start

```bash
# Terminal 1 — run the engine (debug server on by default, port 9876)
cargo run

# Terminal 2 — connect
cargo run -p byro-dbg
```

Custom port on both sides:

```bash
BYRO_DEBUG_PORT=8080 cargo run           # engine
BYRO_DEBUG_PORT=8080 cargo run -p byro-dbg
```

## Example session

```
byro> stats
FPS: 60.2 (avg 59.8) | Frame: 16.61ms | Entities: 1547 | Meshes: 342 | ...

byro> find("TorchSconce01")
  Entity 142 "TorchSconce01"

byro> 142.Transform.translation
[1024.0, 512.0, 128.0]

byro> tex.missing
17 unique missing textures: ...

byro> screenshot /tmp/frame.png
Screenshot saved: /tmp/frame.png
```

## Common workflows

### Pick a reference, inspect it, frame it

Bethesda-console-style `prid` + `inspect` + camera teleport. `prid`
sets a world-scoped `SelectedRef`; follow-up commands fall back to
it when called with no arg.

```
byro> entities Inventory      # list NPCs that have an outfit
byro> prid 12                 # pick one — "selected: entity 12 (saadia)"
byro> cam.tp                  # over-the-shoulder framing of the picked ref
byro> inspect                 # dump every registered component on 12
byro> skin.coverage           # verify RT skinning lands on this viewpoint
```

### Renderer telemetry at a glance

```
byro> ctx.scratch             # per-Vec scratch growth (R6 — catches M40 leaks)
byro> skin.coverage           # green-bar: `coverage: full`
byro> sys.accesses            # R7 — pre-flight for M27 parallel scheduler
```

### Diagnose a "chrome posterized" interior

Per the project's `feedback_chrome_means_missing_textures` memory:
when an interior reads as banded/chrome, run `tex.missing` first —
usually the magenta-checker placeholder × the (correctly loaded)
tangent-space normal map, not a lighting bug.

```
byro> tex.missing             # any missing entries? load the right BSA
byro> mesh.cache              # hit rate sanity check
```

## Full docs

See [`docs/engine/debug-cli.md`](../../docs/engine/debug-cli.md) for
the wire protocol, expression language, registered components +
commands, picked-ref / `inspect` deep dive, camera control, renderer
telemetry, canonical workflows, screenshot pipeline, and
feature-gating details.

## Client-side commands (no network round-trip)

| Command | Action |
|---------|--------|
| `.help` | Print expression-language cheat sheet |
| `quit` / `exit` / `q` (and `.quit` / `.exit` / `.q`) | Exit the REPL |
