#!/usr/bin/env bash
# Headless real-data gate for the load-order fold (PRD legacy-mod-compatibility LMC-REC-002, -004, -007, -011, #6670):
# `lmc-fold-conformance.sh`. No window and no Vulkan.
#
# For each installed game, folds its official load order (masters and DLC, then the installed Creation Club plugins in
# `<game>.ccc` order, masters hoisted first) with the production fold, and diffs the result against
# `scripts/lmc-fold-oracle.py`, which reads the same plugins raw and applies the game's rule: the last plugin carrying a
# record decides it, a deleted winner removes it, a placed reference sits in the cell its winning copy is authored under.
# A form, reference or navmesh the fold gets wrong, a provenance winner that is not the last carrier, or a tombstone any
# record category still holds fails the gate. FormIDs whose mod index points past the master list are reported as known
# (#7329) and do not fail it.
#
# Games: FO3 (BYROREDUX_FO3_DATA), FNV (BYROREDUX_FNV_DATA), Skyrim SE (BYROREDUX_SKYRIMSE_DATA), FO4
# (BYROREDUX_FO4_DATA); a game whose base master is absent is skipped. BYRO_FOLD_PROFILES adds load orders of your own,
# comma-separated `<game>:<file with one plugin path per line>` (fo3, fnv, sse or fo4), e.g. a mod profile from
# `byroredux --import-load-order`. The engine's test binary is built with cargo (CARGO_TARGET_DIR is honoured) unless
# BYRO_TEST_BIN names one. Exit 0 every load order conforms, 1 one does not, 77 nothing to run.
set -euo pipefail

GATE=lmc-fold-conformance
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
STEAM=/mnt/data/SteamLibrary/steamapps/common
command -v python3 >/dev/null || { echo "smoke[$GATE]: SKIP -- missing python3"; exit 77; }

WORK="$(mktemp -d "/tmp/byro-$GATE.XXXXXX")"
# One `<game> <list file>` line per load order to check.
python3 -I - "$WORK" "${BYRO_FOLD_PROFILES:-}" \
    "fo3=${BYROREDUX_FO3_DATA:-$STEAM/Fallout 3 goty/Data}" \
    "fnv=${BYROREDUX_FNV_DATA:-$STEAM/Fallout New Vegas/Data}" \
    "sse=${BYROREDUX_SKYRIMSE_DATA:-$STEAM/Skyrim Special Edition/Data}" \
    "fo4=${BYROREDUX_FO4_DATA:-$STEAM/Fallout 4/Data}" > "$WORK/orders.txt" <<'EOF'
import os, struct, sys
work, profiles, dirs = sys.argv[1], sys.argv[2], dict(a.split('=', 1) for a in sys.argv[3:])
OFFICIAL = {
    'fo3': (['Fallout3.esm', 'Anchorage.esm', 'ThePitt.esm', 'BrokenSteel.esm', 'PointLookout.esm', 'Zeta.esm'], None),
    'fnv': (['FalloutNV.esm', 'DeadMoney.esm', 'HonestHearts.esm', 'OldWorldBlues.esm', 'LonesomeRoad.esm',
             'GunRunnersArsenal.esm', 'ClassicPack.esm', 'MercenaryPack.esm', 'TribalPack.esm', 'CaravanPack.esm'], None),
    'sse': (['Skyrim.esm', 'Update.esm', 'Dawnguard.esm', 'HearthFires.esm', 'Dragonborn.esm'], 'Skyrim.ccc'),
    'fo4': (['Fallout4.esm', 'DLCRobot.esm', 'DLCworkshop01.esm', 'DLCCoast.esm', 'DLCworkshop02.esm',
             'DLCworkshop03.esm', 'DLCNukaWorld.esm', 'DLCUltraHighResolution.esm'], 'Fallout4.ccc'),
}

def masters(path):
    with open(path, 'rb') as f:
        head = f.read(24)
        data = f.read(struct.unpack_from('<I', head, 4)[0])
    out, i = [], 0
    while i + 6 <= len(data):
        tag, size = data[i:i + 4], struct.unpack_from('<H', data, i + 4)[0]
        if tag == b'MAST':
            out.append(data[i + 6:i + 6 + size - 1].decode('cp1252').lower())
        i += 6 + size
    return out

def hoisted(paths):
    """Masters first (LMC-ID-003), as activation orders them before the fold."""
    by_name = {os.path.basename(p).lower(): p for p in paths}
    order, seen = [], set()
    def place(p):
        name = os.path.basename(p).lower()
        if name in seen:
            return
        seen.add(name)
        for m in masters(p):
            if m in by_name:
                place(by_name[m])
        order.append(p)
    for p in paths:
        place(p)
    return order

for game, (base, ccc) in OFFICIAL.items():
    data = dirs[game]
    present = {n.lower(): os.path.join(data, n) for n in os.listdir(data)} if os.path.isdir(data) else {}
    if base[0].lower() not in present:
        continue
    names = [n for n in base if n.lower() in present]
    ccc_path = os.path.join(os.path.dirname(data.rstrip('/')), ccc) if ccc else None
    if ccc_path and os.path.isfile(ccc_path):
        names += [l.strip() for l in open(ccc_path, encoding='cp1252') if l.strip() and l.strip().lower() in present]
    list_file = os.path.join(work, game + '.txt')
    with open(list_file, 'w') as f:
        f.write('\n'.join(hoisted([present[n.lower()] for n in names])) + '\n')
    print(game, list_file)
for entry in filter(None, profiles.split(',')):
    game, _, list_file = entry.partition(':')
    print(game, os.path.abspath(list_file))
EOF
if [[ ! -s "$WORK/orders.txt" ]]; then
    echo "smoke[$GATE]: SKIP -- no game data (set BYROREDUX_FO3_DATA / _FNV_DATA / _SKYRIMSE_DATA / _FO4_DATA, or BYRO_FOLD_PROFILES)"
    exit 77
fi

BIN="${BYRO_TEST_BIN:-}"
if [[ -z "$BIN" ]]; then
    echo "smoke[$GATE]: building the engine test binary"
    BIN="$(cd "$ROOT" && cargo test -p byroredux --bin byroredux --no-run --message-format=json 2>/dev/null \
        | python3 -I -c 'import json, sys
for line in sys.stdin:
    m = json.loads(line)
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "byroredux":
        print(m["executable"])' | tail -1)"
fi
if [[ -z "$BIN" || ! -x "$BIN" ]]; then
    echo "smoke[$GATE]: FAIL -- could not build the engine test binary (cargo test -p byroredux --bin byroredux --no-run)"
    exit 1
fi

echo "smoke[$GATE]: test binary $BIN, dumps in $WORK"
failed=0
while read -r game list; do
    if python3 -I "$ROOT/scripts/lmc-fold-oracle.py" --game "$game" --plugins "$list" --binary "$BIN" --work "$WORK/$game.$(basename "$list" .txt)"; then
        :
    else
        failed=1
    fi
done < "$WORK/orders.txt"
if (( failed )); then
    echo "smoke[$GATE]: FAIL"
    exit 1
fi
echo "smoke[$GATE]: PASS"
