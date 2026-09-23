#!/usr/bin/env python3
"""Capture real-content light/actor diagnostics through the local debug protocol.

Run after a release build, under xvfb-run when headless. Only the owned engine
process is changed: local emitter dimmers are set to zero for an on/off capture.
Images require review; a nonempty TLAS alone does not prove correct lighting.
"""

import argparse
import json
import math
import os
from pathlib import Path
import re
import socket
import struct
import subprocess
import time


SCENES = {
    "oblivion": "ICMarketDistrictTheGildedCarafe",
    "fo3": "MegatonMoriartysSaloon",
    "fnv": "GSProspectorSaloonInterior",
    "skyrim_se": "WhiterunBanneredMare",
    "fo4": "DmndDugoutInn01",
    "starfield": "CityCydoniaMainLevel",
}


def receive_exact(connection, size):
    data = bytearray()
    while len(data) < size:
        chunk = connection.recv(size - len(data))
        if not chunk:
            raise RuntimeError("engine closed the debug connection")
        data.extend(chunk)
    return data


def capture_scene(repo, out, game, cell, timeout, actor_count, actor_yaw, actor_rise,
                  actor_distance):
    out.mkdir(parents=True, exist_ok=True)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    env = dict(os.environ, BYRO_DEBUG_PORT=str(port), BYRO_VALIDATION="1",
               BYROREDUX_FIXED_DT="0",
               RUST_LOG="warn,byroredux::app_events=info")
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("XDG_SESSION_TYPE", None)
    command = [str(repo / "target/release/byroredux"), "--game", game,
               "--cell", cell, "--fly", "--upscaler", "taa"]
    transcript = []
    connection = None
    with (out / "engine.log").open("w") as log:
        process = subprocess.Popen(command, cwd=repo, env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"engine exited during loading: {process.returncode}")
                try:
                    connection = socket.create_connection(("127.0.0.1", port), timeout=1)
                    connection.settimeout(timeout)
                    break
                except OSError:
                    time.sleep(0.25)
            if connection is None:
                raise TimeoutError("engine debug server did not become ready")

            def request(cmd, **fields):
                payload = dict(cmd=cmd, **fields)
                encoded = json.dumps(payload).encode()
                response_deadline = time.monotonic() + timeout
                while True:
                    connection.sendall(struct.pack(">I", len(encoded)) + encoded)
                    length = struct.unpack(">I", receive_exact(connection, 4))[0]
                    if length > 64 * 1024 * 1024:
                        raise RuntimeError(f"oversized debug response: {length}")
                    response = json.loads(receive_exact(connection, length))
                    transcript.append({"request": payload, "response": response})
                    if response.get("kind") != "error":
                        return response
                    message = response.get("message", response)
                    # Cold pipeline compilation can exceed the server's five
                    # second command timeout. All commands here are read-only
                    # or idempotent; screenshots are cancelled by the server.
                    if (message == "timeout waiting for engine response"
                            and time.monotonic() < response_deadline):
                        continue
                    raise RuntimeError(message)

            def settle(frames=30):
                for _ in range(frames):
                    request("stats")

            def view(name, filename):
                request("eval", expr=f"render.debug {name}")
                settle()
                request("screenshot", path=str(out / filename))

            def frame_actor(actor):
                response = request("eval", expr=f"cam.tp {actor['id']}")["data"]
                position = re.search(r"entity \d+ at \(([^)]+)\)", response)
                if position is None:
                    raise RuntimeError(f"cannot frame actor: {response}")
                x, y, z = map(float, position.group(1).split(","))
                # cam.tp aims at the feet. Raise the view to the torso and
                # shorten the offset to avoid nearby interior walls.
                if game == "oblivion":
                    rise, distance = 90, 100
                else:
                    rise, distance = 120, 150
                rise = rise if actor_rise is None else actor_rise
                distance = distance if actor_distance is None else actor_distance
                angle = math.radians(actor_yaw)
                x += math.sin(angle) * distance
                z += math.cos(angle) * distance
                request("eval", expr=f"cam.pos {x} {y + rise} {z}")
                request("eval", expr=f"input.look {actor_yaw} 0")
                request("eval", expr="cam.where")

            settle()
            integrity = request("eval", expr="rt.integrity")["data"]
            masks = request("eval", expr="rt.masks")["data"]
            lights = request("eval", expr="light.dump")["data"]
            actors = request("list_entities", component="EquipmentSlots")["entities"]
            admission_limited = "Scene geometry admission limit reached" in (
                out / "engine.log").read_text(errors="replace")
            result = {"game": game, "cell": cell, "integrity": integrity,
                      "masks": masks, "lights": lights, "actors": actors,
                      "geometry_admission_limited": admission_limited,
                      "actor_view": {"yaw": actor_yaw, "rise": actor_rise,
                                     "distance": actor_distance},
                      "command": command}
            (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
            print(f"{game}: loaded, {len(actors)} actors; capturing images", flush=True)
            view("direct_only", "scene-direct.png")
            view("shadow_visibility", "scene-shadow.png")
            # Store viewpoints before toggling lights, keeping the same world,
            # pose and light order in each actor's matched captures.
            for actor in actors[:actor_count]:
                request("walk_entity", entity=actor["id"], max_depth=5)
                frame_actor(actor)
                view("direct_only", f"actor-{actor['id']}-direct.png")
                view("material_role", f"actor-{actor['id']}-roles.png")
                view("shadow_visibility", f"actor-{actor['id']}-shadow.png")
            emitters = request("list_entities", component="LightSource")["entities"]
            for emitter in emitters:
                request("set_field", entity=emitter["id"], component="LightSource",
                        path="dimmer", value=0.0)
            for actor in actors[:actor_count]:
                frame_actor(actor)
                view("direct_only", f"actor-{actor['id']}-local-off.png")
            result["local_emitters"] = len(emitters)
            (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
            return result
        finally:
            (out / "protocol.json").write_text(json.dumps(transcript, indent=2) + "\n")
            if connection is not None:
                connection.close()
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=20)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("games", nargs="*", default=list(SCENES))
    parser.add_argument("--cell", help="override the cell (one game only)")
    parser.add_argument("--output", default="target/lighting-game-matrix")
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--actors", type=int, default=2)
    parser.add_argument("--actor-yaw", type=float, default=0,
                        help="orbit angle around each actor, in degrees")
    parser.add_argument("--actor-rise", type=float,
                        help="camera height above the actor root, in engine units")
    parser.add_argument("--actor-distance", type=float,
                        help="horizontal camera distance from the actor root")
    args = parser.parse_args()
    if any(game not in SCENES for game in args.games) or (args.cell and len(args.games) != 1):
        parser.error("select known game profiles; --cell requires exactly one game")
    repo = Path(__file__).resolve().parents[1]
    output = (repo / args.output).resolve()
    failed = False
    for game in args.games:
        print(f"capturing {game}", flush=True)
        try:
            result = capture_scene(repo, output / game, game, args.cell or SCENES[game],
                                   args.timeout, args.actors, args.actor_yaw,
                                   args.actor_rise, args.actor_distance)
            print(result["integrity"], flush=True)
            print(result["masks"], flush=True)
            if ("verdict=PASS" not in result["integrity"] or not result["actors"]
                    or result["geometry_admission_limited"]):
                failed = True
                print(f"{game}: incomplete geometry/RT membership or no actor fixture", flush=True)
        except (OSError, RuntimeError, TimeoutError) as error:
            failed = True
            print(f"{game}: {error}", flush=True)
    raise SystemExit(1 if failed else 0)


if __name__ == "__main__":
    main()
