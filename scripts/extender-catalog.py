#!/usr/bin/env python3
"""Generate the per-function extender catalog (docs/data/extenders/*.tsv; docs/engine/extender-catalog.md §1).

Sources, pinned so a re-run reproduces the checked-in tables:
  * SKSE64 and F4SE: `scripts/modified/*.psc` (each extender's additions to the vanilla scripts and its own scripts)
    and `<name>_whatsnew.txt`, fetched with `gh api` at the commits below. Only facts are written: names,
    signatures, versions and where each came from. No extender file is copied (the SKSE readme forbids
    redistribution).
  * The NVSE families: the GECK wiki dump's function pages (`{{Function |origin = … |originVersion = …}}`).
  * OBSE: the CS wiki dump's command pages ("A command for [[:Category:Oblivion Script Extender…") and xOBSE's
    `obse_whatsnew.txt`, which lists commands per version.

A function no changelog names gets its family's first release, labelled `first-release`. A changelog line that only
abbreviates a name ("Is/SetPathNodeDisabled", "…/Magnitudes") counts only where an expansion names a catalogued
function exactly. A missed mention can only make a version earlier, never later.

Usage: scripts/extender-catalog.py [--wiki-root /mnt/data/src/reference] [--out docs/data/extenders]
"""

import argparse
import json
import os
import re
import subprocess
import sys

# Each family's version header: its history maps a release to its index (`SKSE_VERSION_RELEASEIDX`), which
# `GetVersionRelease()` answers. The history starts at SKSE64 2.0.19 and F4SE 0.6.21; older releases have no index.
VERSION_HEADERS = {
    "skse64": ("skse64_common/skse_version.h", "SKSE"),
    "f4se": ("f4se_common/f4se_version.h", "F4SE"),
}

PINS = {
    "skse64": ("ianpatt/skse64", "25b72352adb6", "skse64_whatsnew.txt", "1.1.0"),
    "f4se": ("ianpatt/f4se", "6f6a7caa9aea", "f4se_whatsnew.txt", "0.1.0"),
}
XOBSE = ("llde/xOBSE", "obse_whatsnew.txt")

# GECK wiki `origin` -> catalog family. FOSE1 commands are NVSE's from its first release.
NVSE_ORIGINS = {
    "NVSE": "xnvse",
    "FOSE1": "xnvse",
    "JIP": "jip_ln",
    "JohnnyGuitar": "johnnyguitar",
    "ShowOff": "showoff",
    "LU": "lstewieal_tweaks",
    "SUP": "sup",
    "kNVSE": "knvse",
    "MCM": "mcm",
}

COLUMNS = ["family", "script", "function", "signature", "global", "introduced", "introduced_source", "source"]


def gh_raw(repo, path, ref):
    out = subprocess.run(
        ["gh", "api", f"repos/{repo}/contents/{path}?ref={ref}", "-H", "Accept: application/vnd.github.raw"],
        capture_output=True, check=True,
    )
    return out.stdout.decode("utf-8", errors="replace")


def gh_tree(repo, ref):
    out = subprocess.run(["gh", "api", f"repos/{repo}/git/trees/{ref}?recursive=1"], capture_output=True, check=True)
    return [entry["path"] for entry in json.loads(out.stdout)["tree"] if entry["type"] == "blob"]


def resolve_sha(repo, ref):
    out = subprocess.run(["gh", "api", f"repos/{repo}/commits/{ref}", "--jq", ".sha"], capture_output=True, check=True)
    return out.stdout.decode().strip()


NATIVE = re.compile(
    r"^\s*(?:(?P<ret>[A-Za-z_][\w:]*(?:\[\])?)\s+)?Function\s+(?P<name>\w+)\s*\((?P<params>[^)]*)\)(?P<flags>[^;{]*)",
    re.IGNORECASE,
)


def papyrus_natives(text):
    """The native functions a .psc declares: (script, name, signature, global)."""
    script = None
    for line in text.splitlines():
        match = re.match(r"^\s*ScriptName\s+(\w+)", line, re.IGNORECASE)
        if match and script is None:
            script = match.group(1)
        stripped = line.split(";", 1)[0]
        match = NATIVE.match(stripped)
        if not match:
            continue
        flags = match.group("flags").lower().split()
        if "native" not in flags:
            continue
        ret = match.group("ret") or "None"
        params = " ".join(match.group("params").split())
        yield script, match.group("name"), f"{ret} {match.group('name')}({params})", "global" in flags


VERSION_LINE = re.compile(r"^\s*(?:xOBSE\s+)?v?(\d+(?:\.\d+)*(?:\s+beta(?:\s*\d+)?)?[a-z]?)\s*:?\s*$", re.IGNORECASE)


INLINE_VERSION = re.compile(r"^\s*(\d{4}[a-z]?)\s*:\s*(\S.*)$")


def whatsnew_mentions(text, added_only):
    """(version, line) for each changelog line, oldest release first."""
    sections = []
    version = None
    for line in text.splitlines():
        header = VERSION_LINE.match(line)
        if header:
            version = header.group(1)
            sections.append((version, []))
            continue
        # Early OBSE releases put the version and its summary on one line: "0005: fixing math functions, …".
        inline = INLINE_VERSION.match(line)
        if inline:
            version = inline.group(1)
            sections.append((version, []))
            line = inline.group(2)
        if version is None:
            continue
        body = line.strip().lstrip("-").strip()
        if not body:
            continue
        lowered = body.lower()
        if added_only and not lowered.startswith("added"):
            continue
        if lowered.startswith(("fix", "fixed", "bug", "prevent", "suppress", "partially")):
            continue
        sections[-1][1].append(body)
    sections.reverse()
    return [(version, line) for version, lines in sections for line in lines]


def expansions(token):
    """Candidate names a changelog shorthand stands for: `Get/SetX` -> GetX, SetX; `A/B` -> A, B; `X(Y)`."""
    token = token.strip(" ,.;:()")
    out = {token}
    if "/" in token:
        parts = token.split("/")
        out.update(parts)
        # Get/SetCellLighting: the last part's verb is swapped for each earlier one.
        last = parts[-1]
        verb = re.match(r"^(Get|Set|Is|Has|Mod|Add|Remove|Clear|Can)(.*)$", last)
        if verb:
            for part in parts[:-1]:
                out.add(part + verb.group(2))
    paren = re.match(r"^\((\w+)\)(\w+)$", token)
    if paren:
        out.update({paren.group(2), paren.group(1) + paren.group(2)})
    return {name for name in out if re.match(r"^\w+$", name)}


def introduced_versions(mentions, known, papyrus):
    """The earliest release whose changelog names each known function (`known`: {name or Script.name})."""
    found = {}
    for version, line in mentions:
        for raw in re.findall(r"[\w/().]+", line):
            if papyrus and "." in raw:
                # Script/Script2.Func/Func2 -> every script with every function.
                scripts, funcs = raw.split(".", 1)
                for script in expansions(scripts):
                    for func in expansions(funcs):
                        key = f"{script}.{func}".lower()
                        if key in known and key not in found:
                            found[key] = version
            else:
                for name in expansions(raw):
                    key = name.lower()
                    if key in known and key not in found:
                        found[key] = version
    return found


SCRIPT_NAME = re.compile(r"^\s*ScriptName\s+(\w+)(?:\s+extends\s+(\w+))?", re.IGNORECASE | re.MULTILINE)


def release_table(family, repo, pinned, out_dir):
    """`docs/data/extenders/<family>-releases.tsv`: each version the header's history up to the pinned commit declares,
    with its release index (the first commit that declares the pair)."""
    header, prefix = VERSION_HEADERS[family]
    out = subprocess.run(["gh", "api", f"repos/{repo}/commits?path={header}&sha={pinned}&per_page=100", "--paginate", "--jq", ".[].sha"],
                         capture_output=True, check=True)
    seen = {}
    for sha in reversed(out.stdout.decode().split()):
        text = gh_raw(repo, header, sha)
        values = dict(re.findall(r"#define\s+" + prefix + r"_VERSION_(INTEGER(?:_MINOR|_BETA)?|RELEASEIDX)\s+(\d+)", text))
        try:
            version = f"{values['INTEGER']}.{values['INTEGER_MINOR']}.{values['INTEGER_BETA']}"
            release = int(values["RELEASEIDX"])
        except KeyError:
            continue
        seen.setdefault((version, release), sha[:12])
    with open(os.path.join(out_dir, f"{family}-releases.tsv"), "w", encoding="utf-8") as table:
        table.write("version\trelease\tsource\n")
        for (version, release), sha in sorted(seen.items(), key=lambda item: (item[0][1], [int(p) for p in item[0][0].split(".")])):
            table.write(f"{version}\t{release}\t{repo}@{sha}:{header}\n")
    print(f"{family}: {len(seen)} documented release indices")


# SKSE64's form classes: the `kFormType` enum (GetType's answer, in order from 0) and, per class, the components
# Form's natives cast to. SKSE's headers list a class's extra parents as fields under a `// parents` comment.
FORM_HEADERS = ["GameForms.h", "GameObjects.h", "GameReferences.h", "GameFormComponents.h"]
FORM_COMPONENTS = [("full_name", "TESFullName"), ("weight", "TESWeightForm"), ("keywords", "BGSKeywordForm"),
                   ("model", "TESModel")]


def form_types(repo, sha, out_dir):
    """`docs/data/extenders/skse64-form-types.tsv`: each `kFormType`, its record signature and class, and which of the
    components `Form.GetName` / `GetWeight` / `Get*Keyword*` / `*WorldModel*` read the class carries."""
    texts = {name: gh_raw(repo, f"skse64/{name}", sha) for name in FORM_HEADERS}
    bases = {}
    for text in texts.values():
        lines = text.splitlines()
        for i, line in enumerate(lines):
            m = re.match(r"\s*class\s+(\w+)\s*(?::\s*(.*))?$", line)
            if not m or not (line.rstrip().endswith("{") or (i + 1 < len(lines) and lines[i + 1].strip().startswith("{"))):
                continue
            parents = [re.sub(r"<.*", "", b.strip().split()[-1]) for b in (m.group(2) or "").replace("{", "").split(",") if b.strip()]
            depth, in_parents = 0, False
            for body in lines[i + 1:]:
                depth += body.count("{") - body.count("}")
                stripped = body.strip()
                if stripped.startswith("// parents"):
                    in_parents = True
                elif in_parents:
                    field = re.match(r"(\w+)\s+\w+\s*;", stripped)
                    if field:
                        parents.append(field.group(1))
                    else:
                        in_parents = False
                if depth <= 0 and body.strip().startswith("}"):
                    break
            bases[m.group(1)] = parents

    def ancestors(name, seen):
        for parent in bases.get(name, []):
            if parent not in seen:
                seen.add(parent)
                ancestors(parent, seen)
        return seen

    # One enumerator per line, counted from `kFormType_None = 0` up to `kFormType_Max` (an alias, not a type); its
    # comment names the signature and class, and may be empty.
    enum = "kFormType_None" + texts["GameForms.h"].split("kFormType_None", 1)[1].split("kFormType_Max", 1)[0]
    rows = []
    for line in enum.splitlines():
        m = re.match(r"\s*kFormType_\w+[^,]*,[ \t]*(?://[ \t]*(\S*)[ \t]*(\w*))?", line)
        if m:
            rows.append((m.group(1) or "-", m.group(2) or ""))
    with open(os.path.join(out_dir, "skse64-form-types.tsv"), "w", encoding="utf-8") as table:
        table.write("type\tsignature\tclass\t" + "\t".join(column for column, _ in FORM_COMPONENTS) + "\tsource\n")
        for index, (signature, cls) in enumerate(rows):
            has = ancestors(cls, set()) if cls else set()
            flags = ["1" if component in has else "0" for _, component in FORM_COMPONENTS]
            table.write(f"{index}\t{signature}\t{cls or '-'}\t" + "\t".join(flags) + f"\t{repo}@{sha[:12]}:skse64/GameForms.h\n")
    print(f"skse64: {len(rows)} form types")


def papyrus_family(family, out_dir):
    repo, ref, whatsnew, first = PINS[family]
    sha = resolve_sha(repo, ref)
    release_table(family, repo, sha, out_dir)
    if family == "skse64":
        form_types(repo, sha, out_dir)
    tree = gh_tree(repo, sha)
    # The game's script hierarchy, from the vanilla copies and the extender's own scripts: a call on a subclass reaches
    # a function the extender adds to a parent (LMC-SCR-005 resolves calls by it).
    parents = {}
    for path in sorted(p for p in tree if p.startswith(("scripts/vanilla/", "scripts/modified/")) and p.endswith(".psc")):
        match = SCRIPT_NAME.search(gh_raw(repo, path, sha))
        if match and match.group(1).lower() not in {name.lower() for name in parents}:
            parents[match.group(1)] = match.group(2) or ""
    with open(os.path.join(out_dir, f"{family}-scripts.tsv"), "w", encoding="utf-8") as out:
        out.write("script\tparent\n")
        for script in sorted(parents, key=str.lower):
            out.write(f"{script}\t{parents[script]}\n")
    print(f"{family}: {len(parents)} scripts in the hierarchy")
    paths = sorted(path for path in tree if path.startswith("scripts/modified/") and path.endswith(".psc"))
    rows = []
    for path in paths:
        text = gh_raw(repo, path, sha)
        for script, name, signature, is_global in papyrus_natives(text):
            rows.append({"script": script or os.path.basename(path)[:-4], "function": name, "signature": signature,
                         "global": "1" if is_global else "0", "source": f"{repo}@{sha[:12]}:{path}"})
    known = {f"{row['script']}.{row['function']}".lower() for row in rows}
    found = introduced_versions(whatsnew_mentions(gh_raw(repo, whatsnew, sha), added_only=True), known, papyrus=True)
    for row in rows:
        version = found.get(f"{row['script']}.{row['function']}".lower())
        row.update({"family": family, "introduced": version or first,
                    "introduced_source": f"whatsnew:{version}" if version else "first-release"})
    write(out_dir, family, rows)


WIKI_FIELD = lambda name: re.compile(r"\|\s*" + name + r"\s*=\s*([^|}\n]*)", re.IGNORECASE)


def nvse_families(wiki_root, out_dir):
    pages = os.path.join(wiki_root, "geckwiki", "(main)")
    rows = {}
    text_version = re.compile(r"(?:added|introduced|requires)\s+(?:in\s+|by\s+)?x?NVSE\s+v?(\d+(?:\.\d+)*)", re.IGNORECASE)
    for file in sorted(os.listdir(pages)):
        if not file.endswith(".wiki"):
            continue
        text = open(os.path.join(pages, file), encoding="utf-8", errors="replace").read()
        if "{{Function" not in text:
            continue
        origin_match = WIKI_FIELD("origin").search(text)
        origin = origin_match.group(1).strip() if origin_match else ""
        family = NVSE_ORIGINS.get(origin)
        if not family:
            continue
        name = (WIKI_FIELD("name").search(text) or None)
        name = name.group(1).strip() if name else file[:-5]
        ret = WIKI_FIELD("returnType").search(text)
        args = re.findall(r"\|\s*Name\s*=\s*([^|}\n]*)\s*\|\s*Type\s*=\s*([^|}\n]*)", text)
        signature = f"({ret.group(1).strip() if ret else 'void'}) {name} " + " ".join(f"{n.strip()}:{t.strip()}" for n, t in args)
        version = WIKI_FIELD("originVersion").search(text)
        if version and version.group(1).strip():
            introduced, how = version.group(1).strip(), "wiki:originVersion"
        else:
            match = text_version.search(text) if family == "xnvse" else None
            introduced, how = (match.group(1), "wiki:text") if match else ("first-release", "first-release")
        rows.setdefault(family, []).append({"family": family, "script": "-", "function": name, "signature": signature.strip(),
                                            "global": "-", "introduced": introduced, "introduced_source": how,
                                            "source": f"geckwiki:{file}"})
    for family, family_rows in rows.items():
        write(out_dir, family, family_rows)


def obse_family(wiki_root, out_dir):
    pages = os.path.join(wiki_root, "cs-uesp-wiki", "(main)")
    rows = []
    for file in sorted(os.listdir(pages)):
        if not file.endswith(".wiki"):
            continue
        text = open(os.path.join(pages, file), encoding="utf-8", errors="replace").read()
        if not re.search(r"A command for \[\[:Category:Oblivion Script Extender", text):
            continue
        syntax = re.search(r"'''Syntax:'''\s*\n\s*(.+)", text)
        rows.append({"family": "obse", "script": "-", "function": file[:-5], "signature": syntax.group(1).strip() if syntax else "",
                     "global": "-", "source": f"cs-uesp-wiki:{file}"})
    repo, whatsnew = XOBSE
    sha = resolve_sha(repo, "HEAD")
    known = {row["function"].lower() for row in rows}
    found = introduced_versions(whatsnew_mentions(gh_raw(repo, whatsnew, sha), added_only=False), known, papyrus=False)
    for row in rows:
        version = found.get(row["function"].lower())
        row.update({"introduced": version or "0001",
                    "introduced_source": f"whatsnew:{version}" if version else "first-release"})
    write(out_dir, "obse", rows, note=f"versions from {repo}@{sha[:12]}:{whatsnew}")


def write(out_dir, family, rows, note=None):
    os.makedirs(out_dir, exist_ok=True)
    rows = sorted(rows, key=lambda row: (row["script"].lower(), row["function"].lower()))
    seen = set()
    path = os.path.join(out_dir, f"{family}.tsv")
    with open(path, "w", encoding="utf-8") as out:
        out.write("\t".join(COLUMNS) + "\n")
        for row in rows:
            key = (row["script"].lower(), row["function"].lower())
            if key in seen:
                continue
            seen.add(key)
            out.write("\t".join(str(row[column]).replace("\t", " ") for column in COLUMNS) + "\n")
    dated = sum(1 for row in rows if row["introduced_source"] != "first-release")
    print(f"{family}: {len(seen)} functions, {dated} with a documented introducing version" + (f" ({note})" if note else ""))


def main():
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n", 1)[0])
    parser.add_argument("--wiki-root", default="/mnt/data/src/reference")
    parser.add_argument("--out", default="docs/data/extenders")
    args = parser.parse_args()
    for family in PINS:
        papyrus_family(family, args.out)
    nvse_families(args.wiki_root, args.out)
    obse_family(args.wiki_root, args.out)


if __name__ == "__main__":
    sys.exit(main())
