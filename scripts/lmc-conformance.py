#!/usr/bin/env python3
"""Legacy-mod conformance runner (docs/prd/legacy-mod-compatibility.md
LMC-DX-003, §7.3).

Drives the corpus listed in `assets/legacy_mod_compat/corpus.toml` through the
engine, one mod at a time, and reports which acceptance rules each mod passes
and, per mechanism, how many of the mods exercising it pass. Opt-in: the mod
files are supplied locally and never checked in (LMC-LEGAL-001).

Corpus directory (`--corpus-dir`, else BYROREDUX_LMC_CORPUS): one directory per
installed mod, named by its corpus id and laid out as a mod manager deploys a
mod (its plugins at the top, its archives and loose files beside them):

    <corpus>/fnv-yup/YUP - Base Game + All DLC.esm
    <corpus>/fnv-nmc/textures/...

A mod manager's own folders can be linked in under the id
(`ln -s ~/MO2/mods/"YUP 12.0" <corpus>/fnv-yup`).

Per mod, the §7.3 rules:
  1 activate   the profile activates: the engine's `--plugin-report` (headless,
               no window) exits 0, loads each of the mod's plugins and disables
               none; a mod with no plugin has its root opened and kept. The
               profile is the mod's plugins with their masters, found in the
               game's Data directory or in another installed corpus mod, plus
               the installed corpus mods it `requires`. Checking each
               mechanism's disposition needs the preflight (LMC-DX-001):
               until it exists, rule 1 is activation only.
  2 preflight  not measurable yet: needs LMC-DX-001 / LMC-SCR-005
  3 scenario   the mod's acceptance scenario (`[mod.scenario]` in the corpus):
               the engine is launched on the profile with `--bench-hold`, and
               each step's byro-dbg command must print its `expect` text
  4 save       not measurable yet: needs LMC-SAV-002..004 (Phase 7)

A mod passes when every rule passes; it fails when a measured rule fails; it is
incomplete when nothing failed but a rule is unmeasurable, unwritten or skipped.
Only passes count towards the pass rate, so the published rate never runs ahead
of what was checked. A mod whose masters are not installed is `blocked` (a
setup problem, not an engine result) and is left out of the rates.

Usage:
    lmc-conformance.py [--game fnv|skyrim] [--mod ID ...] [--corpus-dir DIR] [--corpus FILE]
                       [--out DIR] [--no-scenarios] [--headless] [--gate]
    lmc-conformance.py --self-test

Exit status: 0 measured (failures included, unless --gate), 1 a failed mod
under --gate (incomplete ones do not fail it), 77 nothing to run (no corpus directory, no installed mod, no game data
or no engine), 2 usage errors. Standard library only; Python 3.11+.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    sys.exit("lmc-conformance: Python 3.11+ is required (tomllib)")

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "assets/legacy_mod_compat/corpus.toml"
CATALOG = ROOT / "assets/legacy_mod_compat/mechanisms.toml"
FIXTURES = ROOT / "docs/smoke-tests/fixtures"
PLUGIN_EXTENSIONS = (".esm", ".esp", ".esl")
# Corpus game → smoke fixture (data directory and archive list; one source of
# truth with the smoke gates).
FIXTURE_OF = {"fnv": "fnv", "skyrim": "skyrim_se"}
RULES = ("activate", "preflight", "scenario", "save")
NOT_MEASURABLE = {
    "preflight": "not measurable yet: needs the preflight (LMC-DX-001, LMC-SCR-005)",
    "save": "not measurable yet: needs profile-change saves (LMC-SAV-002..004)",
}
SKIP = 77


# ── plugin headers ──────────────────────────────────────────────────────────

def plugin_masters(path: Path) -> list[str]:
    """The MAST list of a TES4-family plugin header (Fallout 3 and later: a
    24-byte record header). Raises ValueError on a file that is not one."""
    with open(path, "rb") as f:
        head = f.read(24)
        if len(head) < 24 or head[:4] != b"TES4":
            raise ValueError(f"{path.name}: not a plugin (no TES4 header)")
        size = struct.unpack_from("<I", head, 4)[0]
        body = f.read(size)
    masters, at, big = [], 0, None
    while at + 6 <= len(body):
        tag = body[at:at + 4]
        length = struct.unpack_from("<H", body, at + 4)[0]
        at += 6
        if big is not None:
            length, big = big, None
        if tag == b"XXXX":
            big = struct.unpack_from("<I", body, at)[0]
        elif tag == b"MAST":
            masters.append(body[at:at + length].split(b"\0", 1)[0].decode("cp1252"))
        at += length
    return masters


def root_plugins(root: Path) -> list[Path]:
    """The plugins a mod root ships at its top level, by name."""
    return sorted((p for p in root.iterdir() if p.is_file() and p.suffix.lower() in PLUGIN_EXTENSIONS),
                  key=lambda p: p.name.lower())


# ── profiles ────────────────────────────────────────────────────────────────

@dataclass
class Profile:
    """One mod's activation profile: `data_plugins` are paths in the Data
    directory, `roots` the corpus roots lowest priority first, `root_plugins`
    the bare names of the plugins they contribute, in load order (masters
    first), and `own` the names of the mod's own plugins."""
    data_plugins: list[Path] = field(default_factory=list)
    roots: list[Path] = field(default_factory=list)
    root_plugins: list[str] = field(default_factory=list)
    own: list[str] = field(default_factory=list)
    missing: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def engine_args(self, base_esm: Path) -> list[str]:
        """`--master` for every plugin but the last, `--esm` for the last; Data
        plugins by path, root plugins by bare name (resolved through the
        roots, LMC-ID-012). A mod with no plugin activates on the base master
        alone, its root still listed."""
        plugins = [str(p) for p in self.data_plugins] + self.root_plugins
        if not plugins:
            plugins = [str(base_esm)]
        args: list[str] = []
        for p in plugins[:-1]:
            args += ["--master", p]
        for r in self.roots:
            args += ["--mod-root", str(r)]
        return args + ["--esm", plugins[-1]]


def build_profile(mod: dict, data_dir: Path, installed: dict[str, Path]) -> Profile:
    prof = Profile()
    data_files = {p.name.lower(): p for p in data_dir.iterdir() if p.is_file()} if data_dir.is_dir() else {}
    # Every installed corpus root's plugins, by lower-cased name → (root, file name).
    root_files: dict[str, tuple[Path, str]] = {}
    for root in installed.values():
        for p in root_plugins(root):
            root_files.setdefault(p.name.lower(), (root, p.name))
    own_root = installed[mod["id"]]
    seen: set[str] = set()

    def add_root(root: Path):
        if root not in prof.roots:
            prof.roots.append(root)

    def visit(name: str, path: Path | None, root: Path | None):
        key = name.lower()
        if key in seen:
            return
        seen.add(key)
        if path is None:
            if key in data_files:
                path, root = data_files[key], None
            elif key in root_files:
                root, name = root_files[key]
                path = root / name
            else:
                prof.missing.append(name)
                return
        try:
            masters = plugin_masters(path)
        except (OSError, ValueError) as e:
            prof.notes.append(str(e))
            masters = []
        for m in masters:
            visit(m, None, None)
        if root is None:
            prof.data_plugins.append(path)
        else:
            add_root(root)
            prof.root_plugins.append(path.name)

    for req in mod.get("requires") or []:
        if req in installed and req != mod["id"]:
            for p in root_plugins(installed[req]):
                visit(p.name, p, installed[req])
            add_root(installed[req])
        elif req.startswith(("fnv-", "sky-")) and req not in installed:
            prof.notes.append(f"requires {req}, which is not installed")
    for p in root_plugins(own_root):
        visit(p.name, p, own_root)
        prof.own.append(p.name)
    # The mod's own root last: highest priority, as its manager would deploy it.
    if own_root in prof.roots:
        prof.roots.remove(own_root)
    prof.roots.append(own_root)
    return prof


# ── rule 1: activation ──────────────────────────────────────────────────────

def judge_activation(report: str, status: int, own: list[str], root_label: str) -> tuple[bool, str]:
    """Read `--plugin-report` output: (passed, reason)."""
    if status != 0:
        lines = [l.strip() for l in report.splitlines() if l.strip()]
        why = next((l for l in lines if l.startswith("Error:")), " | ".join(lines[-3:]))
        return False, f"--plugin-report exited {status}: {why}"
    order = {m.group(1).strip().lower() for m in re.finditer(r"^\s+\d+\s{2}(\S.*)$", report, re.M)}
    for name in own:
        if re.search(rf"^disabled: {re.escape(name)} —", report, re.M | re.I):
            line = next(l for l in report.splitlines() if l.lower().startswith(f"disabled: {name.lower()} —"))
            return False, line
        if name.lower() not in order:
            return False, f"{name} is not in the load order"
    if not own:
        m = re.search(rf"^\s+\d+\s{{2}}{re.escape(root_label)}: (.*)$", report, re.M | re.I)
        if not m:
            return False, f"the report lists no mod root {root_label}"
        if "disabled" in m.group(1):
            return False, f"root {root_label}: {m.group(1)}"
        return True, f"root opened: {m.group(1)}"
    return True, f"{len(own)} plugin(s) loaded: {', '.join(own)}"


# ── rule 3: scenario ────────────────────────────────────────────────────────

def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def run_scenario(scenario: dict, engine: Path, dbg: Path, base_args: list[str], work: Path,
                 headless: bool, timeout: float) -> tuple[bool, str]:
    port = free_port()
    env = dict(os.environ, BYRO_DEBUG_PORT=str(port), BYRO_DEBUG_SERVER="1",
               BYROREDUX_SETTINGS_PATH=str(work / "settings.toml"))
    prefix: list[str] = []
    if headless:
        if not shutil.which("xvfb-run"):
            return False, "--headless needs xvfb-run"
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("XDG_SESSION_TYPE", None)
        prefix = ["xvfb-run", "-a"]
    cmd = prefix + [str(engine)] + base_args + list(scenario.get("launch") or []) + \
        ["--bench-frames", str(scenario.get("bench_frames", 60)), "--bench-hold"]
    (work / "scenario.cmd").write_text(" ".join(cmd) + "\n")
    stderr_path = work / "scenario.stderr"
    with open(work / "scenario.stdout", "wb") as out, open(stderr_path, "wb") as err:
        proc = subprocess.Popen(cmd, env=env, stdout=out, stderr=err, start_new_session=True)
    try:
        deadline = time.monotonic() + timeout
        while b"bench-hold:" not in stderr_path.read_bytes():
            if proc.poll() is not None:
                return False, f"engine exited {proc.returncode} before bench-hold (see {stderr_path})"
            if time.monotonic() > deadline:
                return False, f"no bench-hold within {timeout:.0f} s (see {stderr_path})"
            time.sleep(0.5)
        for i, step in enumerate(scenario["step"], 1):
            time.sleep(float(step.get("wait_s", 0)))
            res = subprocess.run([str(dbg)], input=f"{step['cmd']}\n.quit\n", capture_output=True, text=True,
                                 env=dict(os.environ, BYRO_DEBUG_PORT=str(port)), timeout=60)
            text = (res.stdout + res.stderr).replace("\\n", "\n")
            (work / f"step{i}.log").write_text(text)
            if step["expect"] not in text:
                return False, f"step {i} `{step['cmd']}`: expected '{step['expect']}' (see {work}/step{i}.log)"
        return True, f"{len(scenario['step'])} step(s) passed"
    except subprocess.TimeoutExpired:
        return False, "byro-dbg timed out"
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=20)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()


# ── aggregation ─────────────────────────────────────────────────────────────

def mod_verdict(rules: dict[str, dict]) -> str:
    """`blocked` (the profile does not resolve), `fail` (a measured rule
    failed), `pass` (every rule passed), else `incomplete` (nothing failed, but
    a rule is unmeasurable, unwritten or skipped)."""
    statuses = [r["status"] for r in rules.values()]
    if rules["activate"]["status"] == "blocked":
        return "blocked"
    if "fail" in statuses:
        return "fail"
    return "pass" if all(st == "pass" for st in statuses) else "incomplete"


def summarise(results: list[dict], corpus: list[dict]) -> dict:
    """Per game and per mechanism: mods run, activated and fully passed.
    A mechanism passes only when every corpus mod exercising it is installed
    and passes (§7.3); blocked mods are left out of the rates."""
    by_id = {r["id"]: r for r in results}
    games: dict[str, dict] = {}
    for m in corpus:
        g = games.setdefault(m["game"], {"listed": 0, "run": 0, "blocked": 0, "activated": 0, "passed": 0,
                                         "mechanisms": {}})
        g["listed"] += 1
        r = by_id.get(m["id"])
        activated = passed = scored = False
        if r is not None:
            g["run"] += 1
            g["blocked"] += r["verdict"] == "blocked"
            scored = r["verdict"] != "blocked"
            activated = scored and r["rules"]["activate"]["status"] == "pass"
            passed = scored and r["verdict"] == "pass"
        g["activated"] += activated
        g["passed"] += passed
        for mech in m["mechanisms"]:
            row = g["mechanisms"].setdefault(mech, {"mods": 0, "measured": 0, "activated": 0, "passed": 0})
            row["mods"] += 1
            row["measured"] += scored
            row["activated"] += activated
            row["passed"] += passed
    for g in games.values():
        for row in g["mechanisms"].values():
            row["status"] = "pass" if row["passed"] == row["mods"] else "open"
    return games


def print_summary(results: list[dict], games: dict) -> None:
    for r in results:
        print(f"lmc-conformance: {r['id']}: {r['verdict'].upper()}")
        for rule in RULES:
            st = r["rules"][rule]
            print(f"    {rule:9} {st['status']:8} {st['detail']}")
        for n in r.get("notes", []):
            print(f"    note: {n}")
    for game, g in games.items():
        scored = g["run"] - g["blocked"]
        print(f"lmc-conformance: {game}: ran {g['run']} of {g['listed']} listed, {g['blocked']} blocked; "
              f"activated {g['activated']}/{scored}, passed {g['passed']}/{scored}")
        for mech in sorted(g["mechanisms"], key=lambda k: (k[0], int(re.sub(r'\D', '', k) or 0))):
            row = g["mechanisms"][mech]
            print(f"    {mech:4} {row['status']:4}  mods {row['mods']}, measured {row['measured']}, "
                  f"activated {row['activated']}, passed {row['passed']}")


# ── driver ──────────────────────────────────────────────────────────────────

def fixture(game: str) -> tuple[Path, list[str]]:
    """The game's Data directory and archive arguments, from its smoke fixture."""
    env_file = FIXTURES / f"{FIXTURE_OF[game]}.env"
    script = (f'source "{env_file}"; printf "%s\\0" "${{FIXTURE_DATA_ENV}}" "${{FIXTURE_DATA_DEFAULT}}" '
              f'"${{FIXTURE_ESM}}" "${{FIXTURE_ARCHIVE_ARGS[@]}}"')
    out = subprocess.run(["bash", "-c", script], capture_output=True, check=True).stdout.decode().split("\0")[:-1]
    data_env, data_default, esm, archives = out[0], out[1], out[2], out[3:]
    data = Path(os.environ.get(data_env) or data_default)
    args = [a if a.startswith("--") else str(data / a) for a in archives]
    return data / esm, args


def find_binary(name: str, explicit: str | None) -> Path | None:
    if explicit:
        return Path(explicit) if Path(explicit).is_file() else None
    for d in ("release", "debug"):
        p = ROOT / "target" / d / name
        if p.is_file() and os.access(p, os.X_OK):
            return p
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description="Legacy-mod conformance runner (LMC-DX-003).")
    ap.add_argument("--game", choices=sorted(FIXTURE_OF), action="append")
    ap.add_argument("--mod", action="append", help="corpus id (repeatable); default every installed mod")
    ap.add_argument("--corpus-dir", default=os.environ.get("BYROREDUX_LMC_CORPUS"))
    ap.add_argument("--corpus", type=Path, default=CORPUS,
                    help="corpus file (default the checked-in one; another to draft scenarios against)")
    ap.add_argument("--out", help="work directory (default a new /tmp/byro-lmc-conformance.*)")
    ap.add_argument("--no-scenarios", action="store_true", help="rule 1 only: no window, no Vulkan")
    ap.add_argument("--headless", action="store_true", help="run scenarios under xvfb-run")
    ap.add_argument("--timeout", type=float, default=300.0, help="seconds to reach bench-hold")
    ap.add_argument("--gate", action="store_true", help="exit 1 when an installed mod fails")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()

    if not a.corpus_dir or not Path(a.corpus_dir).is_dir():
        print("lmc-conformance: SKIP -- no corpus directory (--corpus-dir or BYROREDUX_LMC_CORPUS)")
        return SKIP
    corpus_dir = Path(a.corpus_dir).resolve()
    engine = find_binary("byroredux", os.environ.get("BYRO_BIN"))
    if engine is None:
        print("lmc-conformance: SKIP -- missing engine binary (cargo build -p byroredux, or set BYRO_BIN)")
        return SKIP
    dbg = find_binary("byro-dbg", os.environ.get("BYRO_DBG_BIN"))
    corpus = tomllib.loads(a.corpus.read_text(encoding="utf-8"))["mod"]
    games = a.game or sorted(FIXTURE_OF)
    corpus = [m for m in corpus if m["game"] in games]
    unknown = set(a.mod or []) - {m["id"] for m in corpus}
    if unknown:
        print(f"lmc-conformance: unknown corpus id(s) for {games}: {', '.join(sorted(unknown))}", file=sys.stderr)
        return 2
    # Not resolved: the engine names a root by its directory as given (LMC-ID-012), so a linked mod-manager
    # folder keeps its corpus id.
    installed = {m["id"]: corpus_dir / m["id"] for m in corpus if (corpus_dir / m["id"]).is_dir()}
    targets = [m for m in corpus if m["id"] in installed and (not a.mod or m["id"] in a.mod)]
    if not targets:
        print(f"lmc-conformance: SKIP -- no corpus mod for {games} is installed under {corpus_dir}")
        return SKIP
    work = Path(a.out) if a.out else Path(tempfile.mkdtemp(prefix="byro-lmc-conformance."))
    work.mkdir(parents=True, exist_ok=True)
    print(f"lmc-conformance: engine {engine}, corpus {corpus_dir}, logs in {work}")

    results = []
    for game in games:
        mods = [m for m in targets if m["game"] == game]
        if not mods:
            continue
        base_esm, archive_args = fixture(game)
        if not base_esm.is_file():
            print(f"lmc-conformance: SKIP {game} -- missing {base_esm} (set the fixture's data variable)")
            continue
        for mod in mods:
            mwork = work / mod["id"]
            mwork.mkdir(exist_ok=True)
            prof = build_profile(mod, base_esm.parent, installed)
            rules = {k: {"status": "n/a", "detail": v} for k, v in NOT_MEASURABLE.items()}
            if prof.missing:
                rules["activate"] = {"status": "blocked", "detail": "masters not installed: " + ", ".join(prof.missing)}
                rules["scenario"] = {"status": "blocked", "detail": "profile does not resolve"}
            else:
                args = prof.engine_args(base_esm)
                (mwork / "activate.cmd").write_text(" ".join([str(engine)] + args + ["--plugin-report"]) + "\n")
                res = subprocess.run([str(engine)] + args + ["--plugin-report"], capture_output=True, text=True,
                                     timeout=1800)
                report = res.stdout + res.stderr
                (mwork / "activate.log").write_text(report)
                ok, why = judge_activation(report, res.returncode, prof.own, installed[mod["id"]].name.lower())
                rules["activate"] = {"status": "pass" if ok else "fail", "detail": why}
                scenario = mod.get("scenario")
                if not isinstance(scenario, dict):
                    rules["scenario"] = {"status": "missing", "detail": "no acceptance scenario written (§7.3 rule 3)"}
                elif a.no_scenarios:
                    rules["scenario"] = {"status": "skipped", "detail": "--no-scenarios"}
                elif dbg is None:
                    rules["scenario"] = {"status": "skipped", "detail": "missing byro-dbg (cargo build -p byro-dbg)"}
                elif not ok:
                    rules["scenario"] = {"status": "fail", "detail": "the profile did not activate"}
                else:
                    sok, swhy = run_scenario(scenario, engine, dbg, args + archive_args, mwork, a.headless,
                                             a.timeout)
                    rules["scenario"] = {"status": "pass" if sok else "fail", "detail": swhy}
            rules = {k: rules[k] for k in RULES}
            results.append({"id": mod["id"], "game": game, "verdict": mod_verdict(rules), "rules": rules,
                            "profile": {"data_plugins": [p.name for p in prof.data_plugins],
                                        "roots": [r.name for r in prof.roots],
                                        "root_plugins": prof.root_plugins},
                            "notes": prof.notes})
    if not results:
        return SKIP
    summary = summarise(results, [m for m in corpus if m["game"] in {r["game"] for r in results}])
    (work / "results.json").write_text(json.dumps({"mods": results, "games": summary}, indent=2) + "\n")
    print_summary(results, summary)
    print(f"lmc-conformance: results in {work / 'results.json'}")
    failed = any(r["verdict"] == "fail" for r in results)
    return 1 if (a.gate and failed) else 0


# ── self-test ───────────────────────────────────────────────────────────────

def _plugin(masters: list[str]) -> bytes:
    def sub(tag: bytes, data: bytes) -> bytes:
        return tag + struct.pack("<H", len(data)) + data
    body = sub(b"HEDR", struct.pack("<fII", 1.34, 0, 0x800))
    for m in masters:
        body += sub(b"MAST", m.encode() + b"\0") + sub(b"DATA", b"\0" * 8)
    return b"TES4" + struct.pack("<III", len(body), 0, 0) + b"\0" * 8 + body


def self_test() -> int:
    ok = True

    def check(label: str, cond: bool, got=None):
        nonlocal ok
        if not cond:
            ok = False
            print(f"self-test FAIL: {label}" + (f": got {got!r}" if got is not None else ""))

    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)
        data, corpus = t / "Data", t / "corpus"
        for d in (data, corpus / "fnv-a", corpus / "fnv-b", corpus / "fnv-tex" / "textures", corpus / "fnv-c"):
            d.mkdir(parents=True)
        (data / "FalloutNV.esm").write_bytes(_plugin([]))
        (data / "DeadMoney.esm").write_bytes(_plugin(["FalloutNV.esm"]))
        (corpus / "fnv-a" / "Shared.esm").write_bytes(_plugin(["FalloutNV.esm"]))
        (corpus / "fnv-b" / "ModB.esp").write_bytes(_plugin(["FALLOUTNV.ESM", "DeadMoney.esm", "shared.esm"]))
        (corpus / "fnv-c" / "ModC.esp").write_bytes(_plugin(["FalloutNV.esm", "Absent.esm"]))
        (corpus / "fnv-tex" / "textures" / "x.dds").write_bytes(b"x")
        check("MAST list read", plugin_masters(corpus / "fnv-b" / "ModB.esp") == ["FALLOUTNV.ESM", "DeadMoney.esm",
                                                                                 "shared.esm"])
        installed = {i: (corpus / i) for i in ("fnv-a", "fnv-b", "fnv-c", "fnv-tex")}
        p = build_profile({"id": "fnv-b", "requires": ["fnv-tex", "fnv-zzz"]}, data, installed)
        check("data masters by case-folded name, in master order",
              [x.name for x in p.data_plugins] == ["FalloutNV.esm", "DeadMoney.esm"], p.data_plugins)
        check("a master from another corpus root", p.root_plugins == ["Shared.esm", "ModB.esp"], p.root_plugins)
        check("roots: required and master roots first, own root last",
              [r.name for r in p.roots] == ["fnv-tex", "fnv-a", "fnv-b"], p.roots)
        check("a required mod not installed is noted", any("fnv-zzz" in n for n in p.notes), p.notes)
        args = p.engine_args(data / "FalloutNV.esm")
        check("engine args: last plugin is --esm, roots listed",
              args[-2:] == ["--esm", "ModB.esp"] and args.count("--mod-root") == 3, args)
        c = build_profile({"id": "fnv-c"}, data, installed)
        check("a missing master blocks the profile", c.missing == ["Absent.esm"], c.missing)
        tex = build_profile({"id": "fnv-tex"}, data, installed)
        check("an asset-only mod activates on the base master",
              tex.engine_args(data / "FalloutNV.esm")[-2:] == ["--esm", str(data / "FalloutNV.esm")])

    good = ("load order (3):\n    0  FalloutNV.esm\n    1  Shared.esm\n    2  ModB.esp\n"
            "mod roots (1), lowest priority first:\n    0  fnv-tex: 0 plugins (0 in the profile), 0 archives, 1 loose file\n")
    check("activation passes", judge_activation(good, 0, ["ModB.esp"], "x")[0])
    check("a non-zero exit fails", not judge_activation(good, 1, ["ModB.esp"], "x")[0])
    err = judge_activation("noise\nError: cannot be activated\nmore\n", 1, ["ModB.esp"], "x")[1]
    check("a failure names the engine's Error line", err.endswith("Error: cannot be activated"), err)
    check("a plugin absent from the order fails", not judge_activation(good, 0, ["Other.esp"], "x")[0])
    dis = good + "disabled: ModB.esp — its master Absent.esm is not in the profile\n"
    check("a disabled plugin fails", not judge_activation(dis, 0, ["ModB.esp"], "x")[0])
    check("an asset root that opens passes", judge_activation(good, 0, [], "fnv-tex")[0])
    check("an asset root that is disabled fails",
          not judge_activation(good.replace("1 loose file", "disabled with its plugins"), 0, [], "fnv-tex")[0])

    rules_pass = {k: {"status": "pass", "detail": ""} for k in RULES}
    rules_part = dict(rules_pass, save={"status": "n/a", "detail": ""})
    check("all rules pass → pass", mod_verdict(rules_pass) == "pass")
    check("an unmeasurable rule → incomplete", mod_verdict(rules_part) == "incomplete")
    check("a failed rule → fail", mod_verdict(dict(rules_part, scenario={"status": "fail"})) == "fail")
    corpus_rows = [{"id": "a", "game": "fnv", "mechanisms": ["R1", "A2"]},
                   {"id": "b", "game": "fnv", "mechanisms": ["R1"]},
                   {"id": "c", "game": "fnv", "mechanisms": ["A2"]}]
    results = [{"id": "a", "verdict": "pass", "rules": rules_pass},
               {"id": "b", "verdict": "blocked", "rules": dict(rules_pass, activate={"status": "blocked"})}]
    s = summarise(results, corpus_rows)["fnv"]
    check("game counts", (s["listed"], s["run"], s["blocked"], s["passed"]) == (3, 2, 1, 1), s)
    check("a mechanism with an uninstalled mod stays open", s["mechanisms"]["A2"]["status"] == "open", s)
    check("a mechanism with a blocked mod stays open", s["mechanisms"]["R1"]["status"] == "open", s)
    print("lmc-conformance self-test:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
