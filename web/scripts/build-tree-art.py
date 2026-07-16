#!/usr/bin/env python3
"""Build PoB2 passive-tree sprite art for the web viewer.

The vendored PoB2 TreeData ships all tree artwork as zstd-compressed, BC1/BC7
block-compressed DDS *texture arrays* (one array slice per icon/frame). Browsers
can't render those, so this script decodes every passive-node icon and node
frame into WebP offline and emits a manifest the frontend draws as <image>
layers over the existing SVG tree.

Pipeline per atlas: zstd -d -> read DDS header -> for each needed array slice,
splice a 1-slice DDS header + that slice's mip0 -> ImageMagick decodes it.
This reuses magick's BC1/BC7 decoders instead of shipping a decoder in the app.

Requires: zstd, ImageMagick (`magick`), and vendor/PathOfBuilding-PoE2 checked
out (see CLAUDE.md; `bash .claude/skills/run-pobr/driver.sh vendor`).
Run from web/: `npm run build-tree-art`.

Icons/frames sit at each node's own (x,y) which the web passive_tree.json already
provides, and node `skill` ids match the vendor tree, so no coordinate transform
is needed. Group-background discs would need group-center alignment in web space
and are deliberately skipped for now.
# ponytail: emits one WebP per sprite (~530 files), so opening the tree fires
# ~4000 tiny image requests. Fine over HTTP/2 + cache; pack into a single sprite
# sheet + coords JSON (render via <pattern>/<image> sub-rects) if load ever drags.
"""
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile

WEB_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO_ROOT = os.path.dirname(WEB_ROOT)
DATA_ROOT = os.path.join(REPO_ROOT, "data")
VENDOR_TREE = os.path.join(
    REPO_ROOT, "vendor", "PathOfBuilding-PoE2", "src", "TreeData"
)

# Active-node frame overlays only; disabled/dimmed variants are handled with CSS
# opacity on the icon instead of a second art set.
FRAME_KINDS = {
    "normal": "Normal",
    "notable": "Notable",
    "keystone": "Keystone",
    "jewel_socket": "Socket",
}
FRAME_STATES = {"unalloc": "unalloc", "alloc": "alloc"}

# Per-mastery-group icons (MasteryGroupLightning, …) live in the game client, not
# TreeData, so masteries share this generic blank emblem — exactly what PoB2 shows
# for an unallocated mastery. It carries its own frame (no separate overlay).
MASTERY_BLANK = "Art/2DArt/SkillIcons/passives/MasteryBlank.dds"

ATLAS_META_RE = re.compile(r"_(\d+)_(\d+)_(BC\d|RGBA)\.dds\.zst$")
BLOCK_BYTES = {"BC1": 8, "BC7": 16}  # per 4x4 block


def read_version() -> str:
    override = os.environ.get("POBR_DATA_VERSION")
    if override:
        return override
    with open(os.path.join(DATA_ROOT, "CURRENT"), encoding="utf-8") as f:
        return f.read().splitlines()[0].strip()


def pick_tree_version() -> str:
    """Newest vendored tree (0_5, 0_4, ...) — highest number matches latest data."""
    dirs = [d for d in os.listdir(VENDOR_TREE) if re.fullmatch(r"0_\d+", d)]
    if not dirs:
        sys.exit(f"no vendor tree found in {VENDOR_TREE}")
    return max(dirs, key=lambda d: int(d.split("_")[1]))


def atlas_meta(atlas_file: str):
    m = ATLAS_META_RE.search(atlas_file)
    if not m:
        return None
    return int(m.group(1)), int(m.group(2)), m.group(3)


def slice_bytes(w: int, h: int, block: int, mips: int) -> int:
    total, ww, hh = 0, w, h
    for _ in range(mips):
        total += ((ww + 3) // 4) * ((hh + 3) // 4) * block
        ww, hh = max(1, ww // 2), max(1, hh // 2)
    return total


def mip0_bytes(w: int, h: int, block: int) -> int:
    return ((w + 3) // 4) * ((h + 3) // 4) * block


def carve_and_decode(dds: bytes, w: int, h: int, block: int, mips: int,
                     slice_idx: int, out_path: str):
    """Splice a 1-slice DDS around slice `slice_idx` mip0 and let magick decode it.

    ddsCoords indices are 1-based (entry count == arraySize, indices 1..N), so
    convert to a 0-based array slice.
    """
    hdr = bytearray(dds[:148])  # magic(4) + DDS_HEADER(124) + DXT10 header(20)
    struct.pack_into("<I", hdr, 28, 1)   # dwMipMapCount = 1
    struct.pack_into("<I", hdr, 140, 1)  # DXT10 arraySize = 1
    off = 148 + (slice_idx - 1) * slice_bytes(w, h, block, mips)
    data = dds[off:off + mip0_bytes(w, h, block)]
    with tempfile.NamedTemporaryFile(suffix=".dds", delete=False) as tmp:
        tmp.write(hdr + data)
        tmp_path = tmp.name
    try:
        r = subprocess.run(["magick", tmp_path, out_path],
                           capture_output=True, text=True)
        if r.returncode != 0:
            raise RuntimeError(f"magick failed: {r.stderr.strip()[:200]}")
    finally:
        os.unlink(tmp_path)


def main():
    for tool in ("zstd", "magick"):
        if not shutil.which(tool):
            sys.exit(f"missing required tool: {tool}")

    version = read_version()
    tree_ver = os.environ.get("POBR_TREE_VERSION") or pick_tree_version()
    tree_dir = os.path.join(VENDOR_TREE, tree_ver)
    tree = json.load(open(os.path.join(tree_dir, "tree.json"), encoding="utf-8"))
    dds_coords = tree["ddsCoords"]
    node_overlay = tree["nodeOverlay"]
    print(f"data version {version}, vendor tree {tree_ver}")

    # Resolve an asset name/icon-path to the highest-resolution atlas holding it.
    def resolve(name: str):
        best = None  # (size, atlas_file, slice_idx)
        for atlas_file, entries in dds_coords.items():
            if name not in entries or atlas_file.startswith("skills-disabled"):
                continue
            meta = atlas_meta(atlas_file)
            if not meta or meta[2] not in BLOCK_BYTES:
                continue
            size = meta[0] * meta[1]
            if best is None or size > best[0]:
                best = (size, atlas_file, entries[name])
        return None if best is None else (best[1], best[2])

    # Collect wanted slices: node icons + frame overlays. Key = asset name.
    wanted = {}  # name -> (atlas_file, slice_idx)
    node_icons = {}  # skill -> icon path
    for node in tree["nodes"].values():
        icon = node.get("icon")
        skill = node.get("skill")
        if icon is None or skill is None:
            continue
        node_icons[skill] = icon
        if icon not in wanted:
            hit = resolve(icon)
            if hit:
                wanted[icon] = hit

    mastery_hit = resolve(MASTERY_BLANK)
    if mastery_hit:
        wanted[MASTERY_BLANK] = mastery_hit

    frames = {}  # web kind -> {state -> asset name}
    for web_kind, pob_type in FRAME_KINDS.items():
        overlay = node_overlay.get(pob_type)
        if not overlay:
            continue
        frames[web_kind] = {}
        for web_state, pob_state in FRAME_STATES.items():
            name = overlay.get(pob_state)
            if not name:
                continue
            frames[web_kind][web_state] = name
            if name not in wanted:
                hit = resolve(name)
                if hit:
                    wanted[name] = hit

    # Output dir lives outside public/data (which sync-data wipes wholesale).
    out_root = os.path.join(WEB_ROOT, "public", "tree-art", version)
    icons_dir = os.path.join(out_root, "sprites")
    shutil.rmtree(out_root, ignore_errors=True)
    os.makedirs(icons_dir, exist_ok=True)

    def safe_name(asset: str) -> str:
        return re.sub(r"[^A-Za-z0-9]+", "_", asset).strip("_")

    # Decode, grouped by atlas so each .dds.zst is decompressed once.
    asset_file = {}  # asset name -> relative webp path
    by_atlas = {}
    for name, (atlas_file, slice_idx) in wanted.items():
        by_atlas.setdefault(atlas_file, []).append((name, slice_idx))

    done, failed = 0, 0
    for atlas_file, items in by_atlas.items():
        w, h, fmt = atlas_meta(atlas_file)
        block = BLOCK_BYTES[fmt]
        r = subprocess.run(
            ["zstd", "-q", "-f", "-d", os.path.join(tree_dir, atlas_file), "-o",
             os.path.join(tempfile.gettempdir(), "pobr_atlas.dds")],
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            sys.exit(f"zstd failed on {atlas_file}: {r.stderr.strip()[:200]}")
        dds = open(os.path.join(tempfile.gettempdir(), "pobr_atlas.dds"), "rb").read()
        mips = struct.unpack("<I", dds[28:32])[0]
        for name, slice_idx in items:
            rel = f"sprites/{safe_name(name)}.webp"
            try:
                carve_and_decode(dds, w, h, block, mips, slice_idx,
                                 os.path.join(out_root, rel))
                asset_file[name] = rel
                done += 1
            except Exception as e:  # noqa: BLE001 - report and continue
                failed += 1
                print(f"  WARN decode {name}: {e}")
        print(f"  {atlas_file}: {len(items)} slices")

    manifest = {
        "treeVersion": tree_ver,
        "masteryIcon": asset_file.get(MASTERY_BLANK),
        "nodeIcons": {
            str(skill): asset_file[icon]
            for skill, icon in node_icons.items()
            if icon in asset_file
        },
        "frames": {
            kind: {st: asset_file[name] for st, name in states.items()
                   if name in asset_file}
            for kind, states in frames.items()
        },
    }
    with open(os.path.join(out_root, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, separators=(",", ":"))

    print(f"decoded {done} sprites ({failed} failed) -> {out_root}")
    print(f"  nodeIcons: {len(manifest['nodeIcons'])}/{len(node_icons)} nodes")
    print(f"  frames: {sum(len(s) for s in manifest['frames'].values())} overlays")


if __name__ == "__main__":
    main()
