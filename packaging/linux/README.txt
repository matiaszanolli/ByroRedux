ByroRedux -- alpha build for Linux (x86_64)
============================================

A clean Rust + Vulkan rebuild of the Gamebryo / Creation engine lineage
(Oblivion -> Starfield). This is an ALPHA: expect missing features, rendering
glitches and crashes. It ships no Bethesda content -- you need your own legally
obtained copy of a supported game.

What is in this folder
----------------------
  byro-launcher   Window that finds installed games and starts the engine.
  byroredux       The engine itself (what the launcher runs).
  byro-detect     Prints the games it can find on this machine.
  byro-dbg        Console client for a running engine (see below).
  assets/         Game profiles and graphics presets. REQUIRED, read relative
                  to the current directory -- see "Running".
  LICENSE, THIRD_PARTY_NOTICES.md, third_party/   Licences.

What you need
-------------
  * x86_64 Linux with glibc 2.35 or newer.
  * A GPU + driver exposing Vulkan 1.3 and VK_KHR_ray_query (the renderer is
    ray-traced and has no non-RT fallback). 6 GB of VRAM is the realistic floor.
  * Shared libraries: libvulkan.so.1 (the Vulkan loader), libasound.so.2 (ALSA)
    and libstdc++.so.6. On Debian/Ubuntu: libvulkan1 libasound2 libstdc++6.
  * A supported game install: Oblivion, Fallout 3, Fallout: New Vegas,
    Skyrim SE, Fallout 4 or Starfield.

Running
-------
Run everything from inside this folder -- the engine and launcher look for
assets/ in the current directory:

    cd byroredux-*-linux-x86_64
    ./byro-detect            # which games were found?
    ./byro-launcher          # pick a game and press Play

Or start the engine directly, from the game's Data directory (bare archive
names resolve against the current directory), e.g. Fallout: New Vegas:

    cd "/path/to/Fallout New Vegas/Data"
    /path/to/byroredux --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
        --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"

NOTE: started that way, byroredux cannot find assets/debug_profiles.toml unless
your current directory contains an assets/ folder. Copy or symlink this
folder's assets/ next to where you run it, or use the launcher.

    ./byroredux --version    # exactly which build this is

Controls: W A S D + mouse to move, F toggles walk/fly, Space jump/up, Shift
down (fly), Ctrl faster, Escape releases the mouse, Tab opens the inventory.

Debugging a running engine
--------------------------
Release builds only open the debug server when asked:

    BYRO_DEBUG_SERVER=1 ./byroredux ... --bench-hold
    ./byro-dbg               # in a second terminal (port 9876)

Reporting problems
------------------
Please include `./byroredux --version`, your GPU and driver, the game, and the
last lines of the log:   https://github.com/matiaszanolli/ByroRedux/issues
