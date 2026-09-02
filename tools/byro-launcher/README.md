# byro-launcher

The public-facing front end: find installed games, check them, press Play.

```bash
cargo run -p byro-launcher
cargo run -p byro-launcher -- --profiles <path>   # use a different profiles file
```

On Linux, the launcher normally follows the desktop's native window backend.
For X11-only or headless environments that expose an X server alongside a
stale Wayland variable, set `BYROREDUX_LAUNCHER_X11=1`.

## Why it is a separate process on OpenGL

The engine's in-window egui overlay draws through `egui-ash-renderer` *inside
the Vulkan swapchain*, so it cannot render a pixel until the full device chain
succeeds. That makes it useless as the screen where someone fixes a broken GPU
configuration — the people who need that screen are exactly the ones for whom
device creation failed.

So the launcher is its own process on eframe's **glow** (OpenGL) backend. It
opens on machines that fail the engine's Vulkan 1.3 + ray-query requirement,
which is where it must open to explain why, and it adds no second wgpu beside
the one Ruffle already brings.

It also stays resident behind the engine. If the engine dies during startup, the
launcher shows the exit code and the last 200 lines of its stderr — a user who
double-clicked an icon has no terminal to read a panic out of.

## What a Play click does

1. Writes the game's path into the `[roots]` table of the profiles file.
2. Writes a `BootRequest` naming the **profile key**, not paths.
3. Spawns `byroredux --boot <path>`.

Step 2 is deliberate: the engine's own profile expander resolves the ESM and all
five archive categories, so there is one implementation of "which archives does
this game need" rather than a second one living in the launcher.

## Structure

| File | Contains |
|---|---|
| `state.rs` | Every decision — which games are offered, which are launchable, what Play requests. Pure; tested without a window. |
| `engine.rs` | Locating the engine binary and supervising it. Tested against a stub engine. |
| `app.rs` | Layout only. |

The invariant is that `app.rs` holds no logic that is not also reachable from a
unit test without a window or a GPU.

## Scope

P3 of [`docs/engine/launcher.md`](../../docs/engine/launcher.md): Library, Play,
Details, Settings, GPU pre-flight, and detection with a Browse fallback. The
save list (P4) and mod load order (P5) are not implemented; Play offers a
profile's new-game placement and its sample cells.
