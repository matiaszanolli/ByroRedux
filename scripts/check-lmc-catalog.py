#!/usr/bin/env python3
"""Legacy-mod mechanism catalog gate (docs/prd/legacy-mod-compatibility.md,
LMC-CLS-001 / -005 / -006).

`assets/legacy_mod_compat/mechanisms.toml` is the one home of the dispositions.
The PRD's §3.1 tables are generated from it and sit between the
`<!-- lmc-catalog:begin -->` and `<!-- lmc-catalog:end -->` markers; nothing
inside them is edited by hand.

Checks (each is an error):
  C1  the file parses and has `catalog_version` and `games`
  C2  every entry has the required fields with the right types
  C3  ids are unique
  C4  `disposition` and `target` are one of D1..D5; `per_game` keys are games
  C5  `games` is a non-empty subset of the catalog's games
  C6  `reason` and at least one citation are non-empty; `version` is an int >= 1
  C7  a citation naming a repo path points at a file that exists; one naming an
      `LMC-<AREA>-<NNN>` id names one the PRD defines
  C8  every mechanism the PRD refers to as `§3.1 <id>` exists in the catalog
  C9  the generated block in the PRD is up to date

The conformance corpus (PRD §7.2, `assets/legacy_mod_compat/corpus.toml`) is
checked against the catalog:
  C10 the corpus parses, has `corpus_version`, and every mod has the required
      fields with the right types; ids are unique
  C11 `game` is a catalog game; `url` is https; `kind`, `tranche` and `status`
      take their listed values
  C12 every mechanism a mod names exists in the catalog (a bare `N1` matches
      its components `N1.binary` / `N1.facilities`) and is defined for the
      mod's game
  C13 a `verified` mod carries an acceptance scenario and a licence note
      (PRD §7.3 rule 3, LMC-LEGAL-001)
  C14 a scenario has the shape scripts/lmc-conformance.py runs: a `summary`,
      optional `launch` engine arguments and `bench_frames`, and at least one
      `step` with a byro-dbg `cmd`, the `expect` text and an optional `wait_s`

Usage:
    check-lmc-catalog.py            # check
    check-lmc-catalog.py --write    # regenerate the PRD block, then check
    check-lmc-catalog.py --self-test

Standard library only; Python 3.11+ (tomllib).
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    sys.exit("check-lmc-catalog: Python 3.11+ is required (tomllib)")

ROOT = Path(__file__).resolve().parent.parent
CATALOG = Path("assets/legacy_mod_compat/mechanisms.toml")
CORPUS = Path("assets/legacy_mod_compat/corpus.toml")
PRD = Path("docs/prd/legacy-mod-compatibility.md")
BEGIN = "<!-- lmc-catalog:begin -->"
END = "<!-- lmc-catalog:end -->"
DISPOSITIONS = ("D1", "D2", "D3", "D4", "D5")
GROUPS = [
    ("records", "Content: records"),
    ("assets", "Content: assets"),
    ("scripts", "Content: scripts"),
    ("native", "Native code"),
    ("ui", "UI"),
    ("tooling", "Settings and tooling outputs"),
]
GAME_LABEL = {
    "oblivion": "Obl", "fo3": "FO3", "fnv": "FNV",
    "skyrim": "Sky", "fo4": "FO4", "starfield": "SF",
}
REQUIRED = {
    "id": str, "group": str, "name": str, "examples": str, "disposition": str,
    "games": list, "reason": str, "citations": list, "version": int,
}


CORPUS_REQUIRED = {
    "id": str, "game": str, "name": str, "url": str, "author": str, "kind": str,
    "mechanisms": list, "tranche": int, "status": str, "evidence": str,
}
CORPUS_KINDS = ("plugin", "assets", "native", "scripts", "ui", "tool", "total-conversion")
CORPUS_STATUS = ("candidate", "verified")
CORPUS_TRANCHES = (1, 2, 3)


def load(root: Path, path: Path = CATALOG):
    return tomllib.loads((root / path).read_text(encoding="utf-8"))


def validate_corpus(corpus: dict, cat: dict) -> list[str]:
    errors: list[str] = []
    if not isinstance(corpus.get("corpus_version"), int):
        errors.append("C10 `corpus_version` missing or not an integer")
    games = cat.get("games") or []
    mechs = {e.get("id"): e for e in cat.get("mechanism", [])}
    seen: set[str] = set()
    for i, m in enumerate(corpus.get("mod", [])):
        label = m.get("id", f"#{i}")
        for key, typ in CORPUS_REQUIRED.items():
            if not isinstance(m.get(key), typ):
                errors.append(f"C10 {label}: `{key}` missing or not {typ.__name__}")
        if label in seen:
            errors.append(f"C10 {label}: duplicate id")
        seen.add(label)
        game = m.get("game")
        if game not in games:
            errors.append(f"C11 {label}: unknown game {game!r}")
        if not str(m.get("url", "")).startswith("https://"):
            errors.append(f"C11 {label}: `url` must be an https link")
        if m.get("kind") not in CORPUS_KINDS:
            errors.append(f"C11 {label}: `kind` {m.get('kind')!r} is not one of {CORPUS_KINDS}")
        if m.get("tranche") not in CORPUS_TRANCHES:
            errors.append(f"C11 {label}: `tranche` must be one of {CORPUS_TRANCHES}")
        if m.get("status") not in CORPUS_STATUS:
            errors.append(f"C11 {label}: `status` {m.get('status')!r} is not one of {CORPUS_STATUS}")
        if not m.get("mechanisms"):
            errors.append(f"C12 {label}: names no mechanism")
        for mid in m.get("mechanisms") or []:
            entries = [e for k, e in mechs.items() if k == mid or str(k).startswith(mid + ".")]
            if not entries:
                errors.append(f"C12 {label}: mechanism {mid} is not in the catalog")
            elif not any(game in (e.get("games") or []) for e in entries):
                errors.append(f"C12 {label}: mechanism {mid} is not defined for {game}")
        if m.get("status") == "verified":
            if "scenario" not in m:
                errors.append(f"C13 {label}: a verified mod needs `scenario`")
            if not str(m.get("licence", "")).strip():
                errors.append(f"C13 {label}: a verified mod needs `licence`")
        if "scenario" in m:
            errors += [f"C14 {label}: {e}" for e in scenario_errors(m["scenario"])]
    return errors


def scenario_errors(sc) -> list[str]:
    if not isinstance(sc, dict):
        return ["`scenario` must be a table ([mod.scenario])"]
    errors = []
    if not str(sc.get("summary", "")).strip():
        errors.append("scenario needs a `summary`")
    launch = sc.get("launch", [])
    if not isinstance(launch, list) or not all(isinstance(x, str) for x in launch):
        errors.append("scenario `launch` must be a list of strings")
    if "bench_frames" in sc and not (isinstance(sc["bench_frames"], int) and sc["bench_frames"] > 0):
        errors.append("scenario `bench_frames` must be a positive integer")
    steps = sc.get("step")
    if not isinstance(steps, list) or not steps:
        errors.append("scenario needs at least one [[mod.scenario.step]]")
        return errors
    for i, st in enumerate(steps, 1):
        if not isinstance(st, dict):
            errors.append(f"step {i} must be a table")
            continue
        for key in ("cmd", "expect"):
            if not str(st.get(key, "")).strip():
                errors.append(f"step {i} needs `{key}`")
        if "wait_s" in st and not (isinstance(st["wait_s"], (int, float)) and st["wait_s"] >= 0):
            errors.append(f"step {i} `wait_s` must be a non-negative number")
    return errors


def validate(cat: dict, root: Path) -> list[str]:
    errors: list[str] = []
    if not isinstance(cat.get("catalog_version"), int):
        errors.append("C1 `catalog_version` missing or not an integer")
    games = cat.get("games")
    if not isinstance(games, list) or not games:
        errors.append("C1 `games` missing or empty")
        games = []
    mechs = cat.get("mechanism", [])
    prd_text = (root / PRD).read_text(encoding="utf-8")
    prd_ids = set(re.findall(r"\bLMC-[A-Z]+-\d{3}\b", prd_text))
    seen: set[str] = set()
    group_keys = {k for k, _ in GROUPS}
    for i, e in enumerate(mechs):
        label = e.get("id", f"#{i}")
        for key, typ in REQUIRED.items():
            if not isinstance(e.get(key), typ):
                errors.append(f"C2 {label}: `{key}` missing or not {typ.__name__}")
        if label in seen:
            errors.append(f"C3 {label}: duplicate id")
        seen.add(label)
        if e.get("group") not in group_keys:
            errors.append(f"C2 {label}: unknown group {e.get('group')!r}")
        for key in ("disposition", "target"):
            if key in e and e[key] not in DISPOSITIONS:
                errors.append(f"C4 {label}: `{key}` {e[key]!r} is not one of D1..D5")
        for g, d in (e.get("per_game") or {}).items():
            if g not in games:
                errors.append(f"C4 {label}: `per_game` names unknown game {g!r}")
            elif d not in DISPOSITIONS:
                errors.append(f"C4 {label}: `per_game.{g}` {d!r} is not one of D1..D5")
        eg = e.get("games") or []
        if not eg or not set(eg) <= set(games):
            errors.append(f"C5 {label}: `games` must be a non-empty subset of {games}")
        if not str(e.get("reason", "")).strip():
            errors.append(f"C6 {label}: empty reason")
        if not e.get("citations"):
            errors.append(f"C6 {label}: no citation")
        if isinstance(e.get("version"), int) and e["version"] < 1:
            errors.append(f"C6 {label}: version must be >= 1")
        for c in e.get("citations") or []:
            path = c.split("#", 1)[0].split(" ", 1)[0]
            if path.startswith(("docs/", "crates/", "assets/")) and not (root / path).exists():
                errors.append(f"C7 {label}: citation path {path} does not exist")
            for lid in re.findall(r"\bLMC-[A-Z]+-\d{3}\b", c):
                if lid not in prd_ids:
                    errors.append(f"C7 {label}: citation names {lid}, which the PRD does not define")
    for ref in sorted(set(re.findall(r"§3\.1 ([RASNUT]\d+)\b", prd_text))):
        if ref not in seen and not any(s.startswith(ref + ".") for s in seen):
            errors.append(f"C8 the PRD refers to §3.1 {ref}, which the catalog lacks")
    return errors


def render_citation(c: str) -> str:
    if c.startswith("http"):
        host = re.sub(r"^https?://(www\.)?", "", c).split("/")[0]
        return f"[{host}]({c})"
    if re.fullmatch(r"#\d+", c):
        return c
    if c.startswith("docs/prd/legacy-mod-compatibility.md "):
        return c.split(" ", 1)[1]
    if c.startswith("docs/"):
        return f"[{c.split('#')[0].rsplit('/', 1)[-1]}](../{c[len('docs/'):]})"
    if c.startswith(("crates/", "assets/")):
        return f"[`{c}`](../../{c})"
    return c


def disposition_cell(e: dict) -> str:
    d = e["disposition"]
    if e.get("target"):
        d = f"{d} → {e['target']}"
    over = e.get("per_game") or {}
    if over:
        d += "; " + ", ".join(f"{GAME_LABEL[g]} {v}" for g, v in over.items())
    return d


def render(cat: dict) -> str:
    games = cat["games"]
    mechs = cat["mechanism"]
    lines = [
        BEGIN,
        f"<!-- generated by scripts/check-lmc-catalog.py from {CATALOG} (catalog_version {cat['catalog_version']}); do not edit -->",
        "",
        "Games: " + ", ".join(f"{GAME_LABEL[g]} = {g}" for g in games)
        + ". A game not listed for a row has no such mechanism.",
        "",
    ]
    for key, title in GROUPS:
        rows = [e for e in mechs if e["group"] == key]
        if not rows:
            continue
        lines += [f"**{title}**", "",
                  "| Id | Mechanism | Examples | Disp. | Games | Reason and citation | v |",
                  "|---|---|---|---|---|---|---|"]
        for e in rows:
            glist = "all" if e["games"] == games else ", ".join(GAME_LABEL[g] for g in e["games"])
            cites = "; ".join(render_citation(c) for c in e["citations"])
            lines.append(
                f"| {e['id']} | {e['name']} | {e['examples']} | {disposition_cell(e)} | {glist} "
                f"| {e['reason']} ({cites}) | {e['version']} |"
            )
        lines.append("")
    open_rows = [e for e in mechs if e.get("verify")]
    lines += ["**Open Phase 0 questions** (a row with one is a starting position, not a confirmed one):", ""]
    for e in open_rows:
        for v in e["verify"]:
            lines.append(f"- {e['id']}: {v}")
    lines += ["", END]
    return "\n".join(lines)


def block_bounds(text: str):
    b, e = text.find(BEGIN), text.find(END)
    if b < 0 or e < 0 or e < b:
        return None
    return b, e + len(END)


def run(root: Path, write: bool) -> list[str]:
    cat = load(root)
    errors = validate(cat, root)
    if not (root / CORPUS).exists():
        errors.append(f"C10 {CORPUS} is missing")
    else:
        errors += validate_corpus(load(root, CORPUS), cat)
    if errors:
        return errors
    prd_path = root / PRD
    text = prd_path.read_text(encoding="utf-8")
    bounds = block_bounds(text)
    if bounds is None:
        return [f"C9 {PRD} has no {BEGIN} … {END} block"]
    new = text[: bounds[0]] + render(cat) + text[bounds[1]:]
    if new != text:
        if write:
            prd_path.write_text(new, encoding="utf-8")
            print(f"check-lmc-catalog: regenerated the §3.1 block in {PRD}")
        else:
            return [f"C9 the §3.1 block in {PRD} is stale; run scripts/check-lmc-catalog.py --write"]
    return []


def self_test() -> int:
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / CATALOG.parent).mkdir(parents=True)
        (root / PRD.parent).mkdir(parents=True)
        (root / "docs/engine").mkdir(parents=True)
        (root / "docs/engine/x.md").write_text("x")
        (root / PRD).write_text(f"LMC-AST-001 and §3.1 R1\n\n{BEGIN}\n{END}\n")
        good = (
            'catalog_version = 1\ngames = ["fnv", "skyrim"]\n[[mechanism]]\n'
            'id = "R1"\ngroup = "records"\nname = "n"\nexamples = "e"\ndisposition = "D1"\n'
            'games = ["fnv"]\nreason = "r"\ncitations = ["docs/engine/x.md", "docs/prd/legacy-mod-compatibility.md LMC-AST-001"]\nversion = 1\n'
        )
        corpus = (
            'corpus_version = 1\n[[mod]]\nid = "m"\ngame = "fnv"\nname = "n"\n'
            'url = "https://example.org/m"\nauthor = "a"\nkind = "plugin"\nmechanisms = ["R1"]\n'
            'tranche = 1\nstatus = "candidate"\nevidence = "e"\n'
        )
        (root / CORPUS).write_text(corpus)
        def check(label: str, text: str, want: str | None):
            nonlocal ok
            (root / CATALOG).write_text(text)
            (root / PRD).write_text(f"LMC-AST-001 and §3.1 R1\n\n{BEGIN}\n{END}\n")
            errs = run(root, write=True) if want is None else validate(load(root), root)
            hit = [e for e in errs if want and e.startswith(want)]
            if (want is None and errs) or (want and not hit):
                ok = False
                print(f"self-test FAIL: {label}: wanted {want}, got {errs}")
        check("conformant catalog passes and regenerates", good, None)
        check("C3 duplicate id", good + good[good.index("[[mechanism]]"):], "C3")
        check("C4 bad disposition", good.replace('disposition = "D1"', 'disposition = "D9"'), "C4")
        check("C5 unknown game", good.replace('games = ["fnv"]', 'games = ["wii"]'), "C5")
        check("C6 empty reason", good.replace('reason = "r"', 'reason = " "'), "C6")
        check("C7 missing path", good.replace("docs/engine/x.md", "docs/engine/nope.md"), "C7")
        check("C7 undefined LMC id", good.replace("LMC-AST-001\"", "LMC-AST-999\""), "C7")
        check("C8 PRD refers to a missing mechanism", good.replace('id = "R1"', 'id = "R2"'), "C8")
        def check_corpus(label: str, text: str, want: str):
            nonlocal ok
            (root / CATALOG).write_text(good)
            (root / CORPUS).write_text(text)
            errs = validate_corpus(load(root, CORPUS), load(root))
            if not any(e.startswith(want) for e in errs):
                ok = False
                print(f"self-test FAIL: {label}: wanted {want}, got {errs}")
        check_corpus("C10 missing field", corpus.replace('author = "a"\n', ""), "C10")
        check_corpus("C11 non-https url", corpus.replace("https://", "http://"), "C11")
        check_corpus("C11 bad status", corpus.replace('"candidate"', '"maybe"'), "C11")
        check_corpus("C12 unknown mechanism", corpus.replace('["R1"]', '["R9"]'), "C12")
        check_corpus("C12 mechanism not defined for the game", corpus.replace('game = "fnv"', 'game = "skyrim"'), "C12")
        check_corpus("C13 verified without scenario", corpus.replace('"candidate"', '"verified"'), "C13")
        scenario = '[mod.scenario]\nsummary = "s"\nlaunch = ["--cell", "X"]\n[[mod.scenario.step]]\ncmd = "c"\nexpect = "e"\n'
        (root / CORPUS).write_text(corpus + scenario)
        if validate_corpus(load(root, CORPUS), load(root)):
            ok = False
            print("self-test FAIL: a well-formed scenario was rejected")
        check_corpus("C14 scenario as a string", corpus + 'scenario = "do it"\n', "C14")
        check_corpus("C14 scenario without steps", corpus + '[mod.scenario]\nsummary = "s"\n', "C14")
        check_corpus("C14 step without expect", corpus + scenario.replace('expect = "e"\n', ""), "C14")
        (root / CORPUS).write_text(corpus)
        # C9: a stale block is reported when not writing
        (root / CATALOG).write_text(good)
        (root / PRD).write_text(f"LMC-AST-001 and §3.1 R1\n\n{BEGIN}\nstale\n{END}\n")
        if not any(e.startswith("C9") for e in run(root, write=False)):
            ok = False
            print("self-test FAIL: C9 stale block not reported")
    print("check-lmc-catalog self-test:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--root", type=Path, default=ROOT)
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    errors = run(a.root, a.write)
    for e in errors:
        print(f"check-lmc-catalog: {e}")
    if not errors:
        n = len(load(a.root)["mechanism"])
        c = len(load(a.root, CORPUS).get("mod", []))
        print(f"check-lmc-catalog: ok — {n} mechanisms, {c} corpus mods, PRD §3.1 block current")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
