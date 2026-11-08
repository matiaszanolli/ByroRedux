#!/usr/bin/env python3
"""Add the Q1 target surface to the M48 per-game menu inventories (#6643, GUI-REG-006).

PRD `docs/prd/game-ui-integration.md` §10 Q1: a menu gets the game's own movie or
document (S1 / S2) when every host call it makes is served (H1) or deliberately
answered (H2); a menu with an unserved call (H3) takes the engine-native page
(S3) when one exists, else it is absent (S4) with the documented fallback. A
"menu is either served or reported, never silently half-alive" (§1), which is
why one H3 call sends the whole menu to its fallback.

Inputs  (docs/data/game-ui/): menu-inventory-<game>.tsv, host-classes-<profile>.tsv
Output  : the same inventories with four columns appended (idempotent):
          target_surface, fallback_surface, surface_basis, host_classes

Run from the repository root:  python3 scripts/game-ui-data/join_surfaces.py
"""

import csv
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

DATA = Path(__file__).resolve().parents[2] / "docs" / "data" / "game-ui"
ADDED = ["target_surface", "fallback_surface", "surface_basis", "host_classes"]

# Engine-native pages that exist today (`GameMenuPage`, crates/debug-ui/src/panels/game_menu.rs):
# Pause, Settings, Inventory, Dialogue, MessageBox, Barter, Crafting. A menu maps to a page
# only when its name says so; nothing is inferred from function.
NATIVE = {
    "barter": "Barter", "negotiate": "Barter", "barter menu": "Barter", "bartermenu": "Barter",
    "inventory": "Inventory", "inventorymenu": "Inventory",
    "message": "MessageBox", "messageboxmenu": "MessageBox", "message box": "MessageBox",
    "pause": "Pause", "pausemenu": "Pause",
    "dialog": "Dialogue", "dialogue menu": "Dialogue", "dialoguemenu": "Dialogue",
    "crafting menu": "Crafting", "craftingmenu": "Crafting",
    "settings": "Settings", "options": "Settings", "settingsmenu": "Settings",
}

HOST_TABLES = {"skyrim-se": "skyrim", "fallout4": "fallout4"}  # inventory suffix -> host-classes suffix
XML_GAMES = {"oblivion", "fo3", "fnv"}


def read(path):
    with open(path, newline="", encoding="utf-8") as f:
        rows = list(csv.reader(f, delimiter="\t"))
    return rows[0], rows[1:]


def write(path, header, rows):
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(header)
        w.writerows(rows)


def native_page(name):
    return NATIVE.get(re.sub(r"\s+", " ", name.strip().lower()))


def classes_by_movie(host_suffix):
    header, rows = read(DATA / f"host-classes-{host_suffix}.tsv")
    col = {h: i for i, h in enumerate(header)}
    out = defaultdict(Counter)
    for r in rows:
        for movie in filter(None, r[col["menus"]].split(";")):
            out[movie.strip().lower()][r[col["class"]]] += 1
    return out


def movie_stem(movie_or_document):
    m = re.search(r"interface\\([^\\]+)\.swf", movie_or_document.lower())
    return m.group(1) if m else None


def surface_for_movie(name, kind, stem, by_movie):
    fallback = native_page(name)
    fb = f"S3:{fallback}" if fallback else "S4:recorded"
    if kind == "hud":
        return "S1", fb, "HUD movie driven by the Scaleform HUD driver", ""
    if kind != "top-level menu":
        return "n/a", "n/a", f"{kind}: not a menu a script opens", ""
    if stem is None:
        return fb.split(":")[0], fb, "no movie of a matching name: nothing to serve", ""
    counts = by_movie.get(stem)
    if not counts:
        return "S1", fb, "movie makes no catalogued host call (nothing unserved)", ""
    summary = " ".join(f"{k}={counts[k]}" for k in sorted(counts))
    if counts.get("H3"):
        return fb.split(":")[0], fb, f"{counts['H3']} unserved (H3) host call(s): the whole menu takes its fallback", summary
    return "S1", fb, "every host call is H1 (served) or H2 (placeholder)", summary


def surface_for_document(name, kind):
    fallback = native_page(name)
    fb = f"S3:{fallback}" if fallback else "S4:recorded"
    if kind in ("top-level menu", "hud"):
        return "S2", fb, "MenuXml document under the game's MenuProfile (Q1: the game's own document)", ""
    return "n/a", "n/a", f"{kind}: not a menu a script opens", ""


def main():
    for path in sorted(DATA.glob("menu-inventory-*.tsv")):
        game = path.stem.removeprefix("menu-inventory-")
        header, rows = read(path)
        header = [h for h in header if h not in ADDED]
        keep = len(header)
        col = {h: i for i, h in enumerate(header)}
        by_movie = classes_by_movie(HOST_TABLES[game]) if game in HOST_TABLES else None
        out = []
        for r in rows:
            r = r[:keep]
            name, kind, doc = r[col["menu"]], r[col["kind"]], r[col["movie_or_document"]]
            if game in XML_GAMES:
                s = surface_for_document(name, kind)
            else:
                s = surface_for_movie(name, kind, movie_stem(doc), by_movie)
            out.append(r + list(s))
        write(path, header + ADDED, out)
        tally = Counter(r[-4] for r in out)
        print(f"{path.name}: {dict(sorted(tally.items()))}")


if __name__ == "__main__":
    sys.exit(main())
