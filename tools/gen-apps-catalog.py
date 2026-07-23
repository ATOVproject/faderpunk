#!/usr/bin/env python3
"""Generate apps-catalog.json from firmware CONFIG blocks (faderpunk/src/apps).

Source of truth for Param schemas = Rust `.add_param(Param::…)` chains.
Used by the preset editor to build complete param UIs (same field defs as
the configurator receives via GetAllApps / demo catalog).
"""
from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
APPS_DIR = ROOT / "faderpunk/src/apps"
MOD_RS = APPS_DIR / "mod.rs"

# Editor slug overrides (module name → editor key)
SLUG = {"heat_pump": "house_pump"}

OUT_PATHS = [
    ROOT / "tools/apps-catalog.json",
    ROOT / "faderpunk-preset-editor/apps-catalog.json",
    pathlib.Path("/Users/kosmar/Projects/faderpunk-scenes/apps-catalog.json"),
]


def extract_balanced(src: str, open_idx: int) -> str | None:
    """Return substring from open_idx '(' through matching ')'."""
    if open_idx >= len(src) or src[open_idx] != "(":
        return None
    depth = 0
    for i in range(open_idx, len(src)):
        c = src[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return src[open_idx : i + 1]
    return None


def config_body(src: str) -> str | None:
    m = re.search(r"pub static CONFIG\s*:", src)
    if not m:
        return None
    start = src.find("Config::new", m.start())
    if start < 0:
        return None
    # End at the semicolon that terminates the builder chain (after last add_param).
    # Walk forward; track that we've seen at least Config::new(...).
    i = start
    last_semi = None
    while i < len(src):
        if src.startswith("Config::new", i) or src.startswith(".add_param", i):
            # skip to opening paren
            p = src.find("(", i)
            if p < 0:
                break
            bal = extract_balanced(src, p)
            if not bal:
                break
            i = p + len(bal)
            # optional whitespace then ; ends the static
            j = i
            while j < len(src) and src[j] in " \t\n\r":
                j += 1
            if j < len(src) and src[j] == ";":
                return src[start : j + 1]
            continue
        i += 1
    return None


def parse_add_params(body: str, consts: dict[str, object] | None = None) -> list[dict]:
    params: list[dict] = []
    for m in re.finditer(r"\.add_param", body):
        p = body.find("(", m.end())
        if p < 0:
            continue
        call = extract_balanced(body, p)
        if not call:
            continue
        inner = call[1:-1].strip()  # Param::…
        params.append(parse_param(inner, consts))
    return params


def resolve_int_expr(expr: str, consts: dict[str, object]) -> int | None:
    expr = expr.strip()
    if re.fullmatch(r"-?\d+", expr):
        return int(expr)
    # e.g. MAX_STEPS as i32
    m = re.fullmatch(r"(\w+)\s+as\s+\w+", expr)
    if m and m.group(1) in consts and isinstance(consts[m.group(1)], int):
        return int(consts[m.group(1)])
    if expr in consts and isinstance(consts[expr], int):
        return int(consts[expr])
    return None


def file_consts(src: str) -> dict[str, object]:
    """Resolve simple `const NAME: … = …` used inside CONFIG (usize / &[&str])."""
    out: dict[str, object] = {}
    for m in re.finditer(
        r"const\s+(\w+)\s*:\s*usize\s*=\s*(\d+)\s*;", src
    ):
        out[m.group(1)] = int(m.group(2))
    for m in re.finditer(
        r"const\s+(\w+)\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\];", src, re.S
    ):
        out[m.group(1)] = re.findall(r'"([^"]*)"', m.group(2))
    return out


def parse_param(inner: str, consts: dict[str, object] | None = None) -> dict:
    consts = consts or {}
    # Param::MidiIn / MidiOut / …
    um = re.match(r"Param::(MidiIn|MidiOut|MidiNrpn|MidiMode|VoltPerOct)\s*$", inner)
    if um:
        return {"tag": um.group(1)}

    um = re.match(
        r'Param::(MidiChannel|MidiCc|MidiNote|bool)\s*\{\s*name:\s*"([^"]*)"',
        inner,
        re.S,
    )
    if um:
        return {"tag": um.group(1), "name": um.group(2)}

    um = re.match(
        r'Param::i32\s*\{\s*name:\s*"([^"]*)"\s*,\s*min:\s*([^,]+)\s*,\s*max:\s*([^,}]+)',
        inner,
        re.S,
    )
    if um:
        lo = resolve_int_expr(um.group(2), consts)
        hi = resolve_int_expr(um.group(3), consts)
        if lo is None or hi is None:
            return {"tag": "None", "raw": inner[:120]}
        return {"tag": "i32", "name": um.group(1), "min": lo, "max": hi}

    um = re.match(
        r'Param::f32\s*\{\s*name:\s*"([^"]*)"\s*,\s*min:\s*([-\d.]+)\s*,\s*max:\s*([-\d.]+)',
        inner,
        re.S,
    )
    if um:
        return {
            "tag": "f32",
            "name": um.group(1),
            "min": float(um.group(2)),
            "max": float(um.group(3)),
        }

    um = re.match(
        r'Param::Enum\s*\{\s*name:\s*"([^"]*)"\s*,\s*variants:\s*([^,}]+)',
        inner,
        re.S,
    )
    if um:
        variants_expr = um.group(2).strip()
        if variants_expr.startswith("&["):
            # full slice may continue past first line — re-search
            um2 = re.search(
                r'variants:\s*&\[(.*?)\]',
                inner,
                re.S,
            )
            variants = re.findall(r'"([^"]*)"', um2.group(1)) if um2 else []
        else:
            name = variants_expr.strip()
            variants = consts.get(name) if isinstance(consts.get(name), list) else None
            if variants is None:
                return {"tag": "None", "raw": inner[:120]}
        return {"tag": "Enum", "name": um.group(1), "variants": list(variants)}

    for tag, prefix in (
        ("Range", r"Range::"),
        ("Color", r"Color::"),
        ("Curve", r"Curve::"),
        ("Waveform", r"Waveform::"),
        ("Note", r"Note::"),
    ):
        um = re.match(
            rf'Param::{tag}\s*\{{\s*name:\s*"([^"]*)"\s*,\s*variants:\s*&\[(.*?)\]',
            inner,
            re.S,
        )
        if um:
            variants = re.findall(rf"{prefix}(\w+)", um.group(2))
            return {"tag": tag, "name": um.group(1), "variants": variants}

    return {"tag": "None", "raw": inner[:120]}


def parse_header(src: str, body: str) -> dict | None:
    m = re.search(r"pub const CHANNELS:\s*usize\s*=\s*(\d+)", src)
    channels = int(m.group(1)) if m else 1
    m = re.search(
        r'Config::new\(\s*"([^"]+)"\s*,\s*"([^"]*)"\s*,\s*Color::(\w+)\s*,\s*AppIcon::(\w+)',
        body,
        re.S,
    )
    if not m:
        return None
    return {
        "channels": channels,
        "name": m.group(1),
        "description": m.group(2),
        "color": m.group(3),
        "icon": m.group(4),
    }


def to_wire(p: dict) -> dict:
    tag = p["tag"]
    if tag in ("MidiIn", "MidiOut", "MidiNrpn", "MidiMode", "VoltPerOct"):
        return {"tag": tag}
    if tag in ("MidiChannel", "MidiCc", "MidiNote", "bool"):
        return {"tag": tag, "value": {"name": p["name"]}}
    if tag == "i32":
        return {
            "tag": "i32",
            "value": {"name": p["name"], "min": p["min"], "max": p["max"]},
        }
    if tag == "f32":
        return {
            "tag": "f32",
            "value": {"name": p["name"], "min": p["min"], "max": p["max"]},
        }
    if tag == "Enum":
        return {"tag": "Enum", "value": {"name": p["name"], "variants": p["variants"]}}
    if tag in ("Range", "Color", "Curve", "Waveform", "Note"):
        return {
            "tag": tag,
            "value": {
                "name": p["name"],
                "variants": [{"tag": v} for v in p["variants"]],
            },
        }
    return {"tag": "None"}


def main() -> int:
    regs = re.findall(r"(\d+)\s*=>\s*(\w+)", MOD_RS.read_text())
    apps = []
    errors = []
    for app_id, mod_name in regs:
        path = APPS_DIR / f"{mod_name}.rs"
        if not path.exists():
            errors.append(f"missing {mod_name}.rs")
            continue
        src = path.read_text()
        body = config_body(src)
        if not body:
            errors.append(f"no CONFIG body: {mod_name}")
            continue
        header = parse_header(src, body)
        if not header:
            errors.append(f"no Config::new header: {mod_name}")
            continue
        consts = file_consts(src)
        raw_params = parse_add_params(body, consts)
        if any(p["tag"] == "None" for p in raw_params):
            errors.append(
                f"unparsed param in {mod_name}: "
                + ", ".join(p.get("raw", "?") for p in raw_params if p["tag"] == "None")
            )
        slug = SLUG.get(mod_name, mod_name)
        apps.append(
            {
                "id": int(app_id),
                "slug": slug,
                "module": mod_name,
                **header,
                "params": [to_wire(p) for p in raw_params],
            }
        )
        print(f"{app_id:>2} {slug:16} params={len(raw_params):2}  {header['name']}")

    payload = {
        "version": 1,
        "source": "faderpunk/src/apps/*.rs CONFIG (tools/gen-apps-catalog.py)",
        "apps": apps,
    }
    text = json.dumps(payload, indent=2) + "\n"
    for out in OUT_PATHS:
        try:
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(text)
            print(f"wrote {out}")
        except OSError as e:
            print(f"skip {out}: {e}", file=sys.stderr)

    # Sanity: echolot = 15, control = 13
    by_slug = {a["slug"]: a for a in apps}
    expect = {"echolot": 15, "control": 13, "super_lfo": 11, "grooves": 11}
    for slug, n in expect.items():
        got = len(by_slug.get(slug, {}).get("params", []))
        if got != n:
            errors.append(f"{slug}: expected {n} params, got {got}")

    if errors:
        print("ERRORS:", file=sys.stderr)
        for e in errors:
            print(" ", e, file=sys.stderr)
        return 1
    print(f"ok: {len(apps)} apps")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
