#!/usr/bin/env python3
"""Capture and read a Starfield material-map acceptance scenario.

docs/prd/starfield-material-maps.md §7.2. This file is the capture script that
section names: it holds the camera poses and the sample points, and reads the
pass rule off the captures. Only A4 (opacity) is here so far.

    python3 scripts/starfield-material-acceptance.py a4 <out-dir>

Needs an installed Starfield, a Vulkan device, `xvfb-run`, Pillow, and release
builds of `byroredux` and `byro-dbg`. Exit status 0 means every sample passed.
Fixtures come from scripts/starfield-material-fixture.py; no game data is
written to <out-dir> beyond the engine's own screenshots.
"""

import importlib.util
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import time

REPO = pathlib.Path(__file__).resolve().parent.parent
DEBUG_PORT = "19878"
SIZE = (1280, 720)

MESH = "meshes\\setdressing\\found\\foundsquaretable_01.nif"
MATERIAL = "materials\\setdressing\\found\\foundsquaretable_01.mat"
# `mat.dump` entity of the table's first shape in the Studio viewer.
ENTITY = 3

# The viewer's own framing looks level at the upright lid. The flat tabletop
# behind it is edge-on there, and casts the floor shadow; this pose, inside the
# room behind the table, looks down at it.
REAR_POSE = ("cam.pos 0 138 -46", "input.look 180 -68")

# Sample points as screen pixels at 1280x720, with the state the checkerboard
# fixture gives the block there: True closed (solid), False open. The inverse
# fixture flips every one.
LID = {  # default pose
    (532, 90): True,
    (640, 190): True,
    (745, 300): True,
    (640, 100): False,
    (532, 190): False,
    (745, 190): False,
    (640, 285): False,
}
# Each floor point (default pose) with a pixel (rear pose) of the tabletop
# block that the segment from the point to the room's key light passes
# through. The right one sits a little off the crossing, clear of a leg.
FLOOR = {
    "centre": ((640, 630), (640, 239), True),
    "left": ((430, 645), (842, 198), False),
    "right": ((845, 650), (480, 190), False),
}

# The opacity view writes without lighting or tone mapping, through the sRGB
# output curve: a closed block holds 1.0; behind an open block the room's wall
# shows, which has no opacity source and draws its colour alpha (1.0) in blue.
CLOSED_RGB = (255, 255, 255)
OPEN_RGB = (0, 0, 255)
VIEW_TOLERANCE = 2
# Floor readings are 9x9 means of the direct-light view, 0-255. "Lit behind an
# open block, shadowed behind a closed one" is read as the open run exceeding
# the closed run by this much. Light also arrives through neighbouring blocks
# and from the fill light, so neither reading is fully lit or fully dark.
FLOOR_MARGIN = 20.0
# The direct-light view samples its lights at random each frame, so a run's
# floor reading is the mean over this many captures. Two identical runs must
# then agree this closely for a floor reading to count.
DIRECT_CAPTURES = 5
FLOOR_NOISE = 2.0

RUNS = (("checker", "opacity"), ("inverse", "opacity-inverse"), ("repeat", "opacity"))


def fixture_module():
    path = REPO / "scripts" / "starfield-material-fixture.py"
    spec = importlib.util.spec_from_file_location("smm_fixture", path)
    assert spec is not None and spec.loader is not None, path
    module = importlib.util.module_from_spec(spec)
    sys.dont_write_bytecode = True
    spec.loader.exec_module(module)
    return module


def console(env, commands):
    """One byro-dbg batch."""
    done = subprocess.run(
        [str(REPO / "target/release/byro-dbg")],
        input="\n".join(commands) + "\n.quit\n",
        env=env, capture_output=True, text=True, timeout=120,
    )
    return done.stdout


def capture(out, tag, fixture_root):
    """One viewer session. Returns the `mat.dump` text."""
    settings = out / f"{tag}.toml"
    settings.write_text("version = 1\n[settings]\n")
    env = dict(os.environ, BYROREDUX_SETTINGS_PATH=str(settings), BYRO_DEBUG_SERVER="1",
               BYRO_DEBUG_PORT=DEBUG_PORT, RUST_LOG="info")
    for key in ("WAYLAND_DISPLAY", "XDG_SESSION_TYPE"):
        env.pop(key, None)
    log = out / f"{tag}.log"
    (out / "screenshots").mkdir(exist_ok=True)
    with open(log, "w") as sink:
        engine = subprocess.Popen(
            ["xvfb-run", "-a", str(REPO / "target/release/byroredux"), "--game", "starfield",
             "--studio", "--mesh", MESH, "--loose-root", str(fixture_root),
             "--bench-frames", "120", "--bench-hold"],
            # `--game` reads assets/debug_profiles.toml from the working directory.
            cwd=REPO, env=env, stdout=sink, stderr=subprocess.STDOUT, start_new_session=True,
        )
    try:
        deadline = time.time() + 300
        while "bench-hold: engine held open" not in log.read_text(errors="replace"):
            if engine.poll() is not None or time.time() > deadline:
                raise SystemExit(f"{tag}: the engine did not come up; see {log}")
            time.sleep(1)
        if f"placed cli \u00b7 {MESH}" not in log.read_text(errors="replace"):
            raise SystemExit(f"{tag}: the viewer did not load {MESH}; see {log}")
        time.sleep(4)
        dump = console(env, [f"mat.dump {ENTITY}", "studio.overlay off"])

        def shot(name, *setup):
            console(env, list(setup))
            time.sleep(4)
            # The engine writes under its working directory; the capture
            # is moved to <out-dir> at once.
            filename = f"smm_a4_{os.getpid()}_{tag}_{name}.png"
            written = REPO / "screenshots" / filename
            console(env, [f"screenshot {filename}"])
            time.sleep(2)
            if not written.exists():
                raise SystemExit(f"{tag}: no capture {written}; see {log}")
            shutil.move(written, out / "screenshots" / f"{tag}_{name}.png")

        shot("opacity", "render.debug material_opacity")
        for number in range(DIRECT_CAPTURES):
            shot(f"direct{number}", "render.debug direct_only")
        shot("final", "render.debug final")
        shot("rear", "render.debug material_opacity", *REAR_POSE)
        return dump
    finally:
        try:
            os.killpg(engine.pid, signal.SIGTERM)
            engine.wait(timeout=30)
        except (ProcessLookupError, subprocess.TimeoutExpired):
            try:
                os.killpg(engine.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def image(out, tag, name):
    from PIL import Image

    picture = Image.open(out / "screenshots" / f"{tag}_{name}.png").convert("RGB")
    if picture.size != SIZE:
        raise SystemExit(f"{tag}_{name}: {picture.size}, the sample points are for {SIZE}")
    return picture


def floor_reading(picture, point):
    x, y = point
    box = [picture.getpixel((i, j)) for i in range(x - 4, x + 5) for j in range(y - 4, y + 5)]
    return sum(0.2126 * r + 0.7152 * g + 0.0722 * b for r, g, b in box) / len(box)


def a4(out):
    if subprocess.run(["pgrep", "-x", "byroredux"], capture_output=True).returncode == 0:
        raise SystemExit("another byroredux is running; not starting a second one")
    fixtures = fixture_module()
    dumps = {}
    for tag, kind in RUNS:
        root = out / f"fixture-{kind}"
        fixtures.write(kind, MATERIAL, root)
        dumps[tag] = capture(out, tag, root)

    failures = []

    def check(label, passed, detail):
        print(f"{'pass' if passed else 'FAIL'}  {label}: {detail}")
        if not passed:
            failures.append(label)

    for tag, kind in RUNS:
        want = f"opacity: Map textures\\smm_fixture\\{kind}_opacity.dds"
        # byro-dbg prints the dump as one quoted string with escapes.
        text = dumps[tag].replace("\\\\", "\\").lower()
        check(f"{tag} mat.dump", want.lower() in text, want)

    def reads(pixel, closed):
        target = CLOSED_RGB if closed else OPEN_RGB
        return all(abs(a - b) <= VIEW_TOLERANCE for a, b in zip(pixel, target))

    def state(closed):
        return "closed" if closed else "open"

    for tag, flip in (("checker", False), ("inverse", True)):
        view, rear = image(out, tag, "opacity"), image(out, tag, "rear")
        for point, closed in LID.items():
            closed ^= flip
            pixel = view.getpixel(point)
            check(f"{tag} lid {point}", reads(pixel, closed), f"{state(closed)}, read {pixel}")
        for name, (_, block, closed) in FLOOR.items():
            closed ^= flip
            pixel = rear.getpixel(block)
            check(f"{tag} tabletop over {name} {block}", reads(pixel, closed),
                  f"{state(closed)}, read {pixel}")

    floors = {
        tag: [image(out, tag, f"direct{number}") for number in range(DIRECT_CAPTURES)]
        for tag, _ in RUNS
    }
    for name, (point, _, closed_in_checker) in FLOOR.items():
        readings = {
            tag: [floor_reading(picture, point) for picture in pictures]
            for tag, pictures in floors.items()
        }
        values = {tag: sum(r) / len(r) for tag, r in readings.items()}
        spread = max(max(r) - min(r) for r in readings.values())
        open_tag, closed_tag = (
            ("inverse", "checker") if closed_in_checker else ("checker", "inverse")
        )
        gap = values[open_tag] - values[closed_tag]
        noise = abs(values["checker"] - values["repeat"])
        check(
            f"floor {name} {point}",
            gap >= FLOOR_MARGIN and noise <= FLOOR_NOISE,
            f"behind open {values[open_tag]:.1f}, behind closed {values[closed_tag]:.1f}, "
            f"gap {gap:+.1f} (need {FLOOR_MARGIN:.0f}), identical runs differ {noise:.1f} "
            f"(single captures within a run spread {spread:.1f})",
        )

    print(f"A4: {'PASS' if not failures else f'{len(failures)} sample(s) FAILED'}")
    return 1 if failures else 0


def main(argv):
    if len(argv) != 2 or argv[0] != "a4":
        print(__doc__, file=sys.stderr)
        return 2
    # A termination request unwinds, so the engine's own session is shut down.
    signal.signal(signal.SIGTERM, lambda number, frame: sys.exit(128 + number))
    out = pathlib.Path(argv[1]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    return a4(out)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
