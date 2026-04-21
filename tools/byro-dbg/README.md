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

byro> mesh.cache
NIF import cache:
  entries:       342 (341 parsed, 1 failed)
  hit rate:      96.4%

byro> screenshot /tmp/frame.png
Screenshot saved: /tmp/frame.png

byro> quit
```

## Full docs

See [`docs/engine/debug-cli.md`](../../docs/engine/debug-cli.md) for the
wire protocol, expression language, registered components, screenshot
pipeline, and feature-gating details.

## Client-side commands (no network round-trip)

| Command | Action |
|---------|--------|
| `.help` | Print expression-language cheat sheet |
| `quit` / `exit` / `q` (and `.quit` / `.exit` / `.q`) | Exit the REPL |
