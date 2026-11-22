#!/usr/bin/env python3
"""Write the four-NPC test plugin of the Oblivion auto-calc timing observation (#7022).

The plugin holds four `NPC_` records cloned from one vanilla NPC, one per combination of the ACBS flags
"Auto-calc stats" (bit 4, 0x10) and "PC Level Offset" (bit 7, 0x80), each with stats that are plainly not what
the calculator produces (every skill and attribute authored 90, Health 999, spell points 777, Fatigue 888).
An NPC whose in-game values still read those numbers was never calculated; one that reads anything else was.

Usage:
    scripts/oblivion-autocalc-plugin.py --data "<Oblivion>/Data" --out Byro7022Autocalc.esp

The protocol that uses it is docs/smoke-tests/oblivion-autocalc-timing.md. Nothing is written into the game
folder, and the vanilla master is only read.
"""
import argparse
import os
import struct
import sys
import zlib

# TES4 (Oblivion) layout: a record header is 20 bytes (type, data size, flags, form id, version control),
# a GRUP header 20 bytes (GRUP, size including the header, label, group type, stamp).
RECORD_HEADER = 20
FLAG_COMPRESSED = 0x00040000
AUTO_CALC = 0x10
PC_LEVEL_OFFSET = 0x80
# Authored values, chosen to differ from any class / race calculation.
SKILL = 90
ATTRIBUTE = 90
HEALTH = 999
SPELL_POINTS = 777
FATIGUE = 888
COMBOS = [
    ("A", "no flag", 0),
    ("B", "auto-calc", AUTO_CALC),
    ("C", "pc-level-offset", PC_LEVEL_OFFSET),
    ("D", "auto-calc + pc-level-offset", AUTO_CALC | PC_LEVEL_OFFSET),
]
FIRST_FORM_ID = 0x01000800  # load-order index 1: the plugin's one master is Oblivion.esm


def read_subrecords(data):
    subs, pos = [], 0
    while pos < len(data):
        kind = data[pos:pos + 4]
        size = struct.unpack_from("<H", data, pos + 4)[0]
        pos += 6
        subs.append((kind, data[pos:pos + size]))
        pos += size
    return subs


def npc_records(buf):
    """Yield (form_id, flags, subrecords) for every NPC_ record of an Oblivion master."""
    pos = 0
    while pos < len(buf):
        kind = buf[pos:pos + 4]
        size = struct.unpack_from("<I", buf, pos + 4)[0]
        if kind == b"GRUP":
            label = buf[pos + 8:pos + 12]
            if label == b"NPC_":
                end, cursor = pos + size, pos + RECORD_HEADER
                while cursor < end:
                    rkind = buf[cursor:cursor + 4]
                    rsize, rflags, rform = struct.unpack_from("<III", buf, cursor + 4)
                    body = buf[cursor + RECORD_HEADER:cursor + RECORD_HEADER + rsize]
                    if rkind == b"NPC_":
                        if rflags & FLAG_COMPRESSED:
                            body = zlib.decompress(body[4:])
                        yield rform, rflags, read_subrecords(body)
                    cursor += RECORD_HEADER + rsize
            pos += size
        else:
            pos += RECORD_HEADER + size


def pick_source(buf, wanted):
    """A plain, scriptless NPC with a race, a class and the full stat block."""
    for form_id, _, subs in npc_records(buf):
        kinds = {k: v for k, v in subs}
        edid = kinds.get(b"EDID", b"").rstrip(b"\0").decode("latin1")
        if wanted and edid != wanted:
            continue
        if wanted or (
            len(kinds.get(b"ACBS", b"")) == 16
            and len(kinds.get(b"DATA", b"")) == 33
            and b"RNAM" in kinds
            and b"CNAM" in kinds
            and b"SCRI" not in kinds
            and b"TPLT" not in kinds
        ):
            return form_id, edid, subs
    sys.exit("no suitable source NPC_ found (pass --source <EDID>)")


def sub(kind, data):
    return kind + struct.pack("<H", len(data)) + data


def clone(subs, edid, full, flags):
    out = []
    for kind, data in subs:
        if kind == b"PKID":
            continue  # no AI packages: the clones stand where they are placed
        if kind == b"EDID":
            data = edid.encode() + b"\0"
        elif kind == b"FULL":
            data = full.encode() + b"\0"
        elif kind == b"ACBS":
            base, _, _, barter, _, _, _ = struct.unpack("<IHHHhHH", data)
            level = 5 if not flags & PC_LEVEL_OFFSET else 2  # a level, or an offset above the player's
            data = struct.pack(
                "<IHHHhHH",
                (base & ~(AUTO_CALC | PC_LEVEL_OFFSET)) | flags,
                SPELL_POINTS,
                FATIGUE,
                barter,
                level,
                0,
                0,
            )
        elif kind == b"DATA":
            data = bytes([SKILL] * 21) + struct.pack("<I", HEALTH) + bytes([ATTRIBUTE] * 8)
        out.append((kind, data))
    if not any(kind == b"FULL" for kind, _ in out):
        out.insert(1, (b"FULL", full.encode() + b"\0"))
    return out


def record(kind, form_id, subs):
    body = b"".join(sub(k, d) for k, d in subs)
    return kind + struct.pack("<IIII", len(body), 0, form_id, 0) + body


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data", default=os.environ.get("BYROREDUX_OBLIVION_DATA"), help="Oblivion Data directory")
    parser.add_argument("--out", default="Byro7022Autocalc.esp")
    parser.add_argument("--source", default="AnvilGuardCityPostDay02", help="EDID of the NPC_ to clone")
    args = parser.parse_args()
    if not args.data:
        sys.exit("pass --data or set BYROREDUX_OBLIVION_DATA")
    master = os.path.join(args.data, "Oblivion.esm")
    buf = open(master, "rb").read()
    src_form, src_edid, src_subs = pick_source(buf, args.source)
    print(f"cloning {src_edid} ({src_form:08X})")

    records = []
    for index, (letter, label, flags) in enumerate(COMBOS):
        edid = f"Byro7022{letter}"
        subs = clone(src_subs, edid, f"Test {letter} {label}", flags)
        form_id = FIRST_FORM_ID + index
        records.append(record(b"NPC_", form_id, subs))
        print(f"  {edid}  {form_id:08X}  ACBS flags {flags:#04x}")
    body = b"".join(records)
    group = b"GRUP" + struct.pack("<I", RECORD_HEADER + len(body)) + b"NPC_" + struct.pack("<iI", 0, 0) + body

    hedr = struct.pack("<fII", 1.0, len(records), FIRST_FORM_ID + len(records))
    header_subs = [
        (b"HEDR", hedr),
        (b"CNAM", b"byroredux #7022\0"),
        (b"SNAM", b"Four NPCs for the auto-calc timing observation\0"),
        (b"MAST", b"Oblivion.esm\0"),
        (b"DATA", struct.pack("<Q", 0)),
    ]
    header_body = b"".join(sub(k, d) for k, d in header_subs)
    tes4 = b"TES4" + struct.pack("<IIII", len(header_body), 0, 0, 0) + header_body
    with open(args.out, "wb") as handle:
        handle.write(tes4 + group)
    print(f"wrote {args.out} ({len(tes4) + len(group)} bytes)")


if __name__ == "__main__":
    main()
