#!/usr/bin/env python3
"""Assemble docs/wip-apps-ux-cheatsheet.html from per-app fragments + legacy HTML.

Sources (merged by app id, apps/ wins over legacy):
  docs/apps/<id>-<slug>/manual.json + manual.md
  docs/cheatsheet/legacy/<id>-*.html

Shell/footer:
  docs/cheatsheet/shell.html
  docs/cheatsheet/footer.html

Usage:
  python3 scripts/assemble-cheatsheet.py
  python3 scripts/assemble-cheatsheet.py --scope wip|stock|all
  python3 scripts/assemble-cheatsheet.py --pdf
"""

from __future__ import annotations

import argparse
import html as html_lib
import json
import re
import subprocess
import sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APPS_DIR = ROOT / "docs" / "apps"
CHEAT_DIR = ROOT / "docs" / "cheatsheet"
LEGACY_DIR = CHEAT_DIR / "legacy"
SHELL = CHEAT_DIR / "shell.html"
FOOTER = CHEAT_DIR / "footer.html"
OUT_HTML = ROOT / "docs" / "wip-apps-ux-cheatsheet.html"
OUT_PDF = ROOT / "docs" / "wip-apps-ux-cheatsheet.pdf"


def load_app_docs() -> dict[int, dict]:
    docs: dict[int, dict] = {}
    if not APPS_DIR.is_dir():
        return docs
    for path in sorted(APPS_DIR.glob("*/manual.json")):
        data = json.loads(path.read_text())
        app_id = int(data["id"])
        md_path = path.with_name("manual.md")
        prose = md_path.read_text().strip() if md_path.exists() else ""
        data["_prose"] = prose
        data["_source"] = "apps"
        docs[app_id] = data
    return docs


def load_legacy() -> dict[int, str]:
    legacy: dict[int, str] = {}
    if not LEGACY_DIR.is_dir():
        return legacy
    for path in sorted(LEGACY_DIR.glob("*.html")):
        text = path.read_text()
        m = re.search(r"ID\s+(\d+)", text)
        if not m:
            raise SystemExit(f"legacy fragment missing ID: {path}")
        legacy[int(m.group(1))] = text if text.endswith("\n") else text + "\n"
    return legacy


def escape_text(s: str) -> str:
    return html_lib.escape(s, quote=False)


def render_section_block(section: dict) -> str:
    items = "\n".join(f"          <li>{item}</li>" for item in section["items"])
    return f"        <h3>{escape_text(section['heading'])}</h3>\n        <ul>\n{items}\n        </ul>\n"


def render_app_html(doc: dict) -> str:
    app_id = int(doc["id"])
    name = doc["name"]
    tag = doc.get("tag", "draft")
    desc = doc.get("description", "")
    comment = name.upper()
    prose = re.sub(r"\s+", " ", doc.get("_prose", "")).strip()
    prose_html = escape_text(prose)

    cols = doc.get("columns") or []
    # Pad / trim to 3 visual columns for the print grid.
    while len(cols) < 3:
        cols.append({"sections": []})
    cols = cols[:3]

    col_html = []
    for col in cols:
        blocks = "".join(render_section_block(s) for s in col.get("sections", []))
        col_html.append(f"      <div>\n{blocks}      </div>\n")

    return (
        f"<!-- {comment} -->\n"
        f'<section class="app">\n'
        f'  <div class="app-head">\n'
        f'    <span class="id">ID {app_id}</span>\n'
        f'    <h2>{escape_text(name)} <span class="tag {tag}">{tag}</span></h2>\n'
        f'    <span class="desc">{escape_text(desc)}</span>\n'
        f"  </div>\n"
        f'  <div class="app-body">\n'
        f'    <div class="cols">\n'
        f"{''.join(col_html)}"
        f"    </div>\n"
        f'    <div class="manual">\n'
        f'      <p><strong>Manual.</strong> {prose_html}</p>\n'
        f"    </div>\n"
        f"  </div>\n"
        f"</section>\n\n"
    )


def patch_shell(shell: str, scope: str, ids: list[int]) -> str:
    lo = min(ids) if ids else 0
    hi = max(ids) if ids else 0
    today = date.today().strftime("%-d %b %Y")
    if scope == "all":
        title = "Faderpunk Apps — UX Cheatsheet"
        sub = f"Stock + WIP (IDs {lo}–{hi}) · {today} · A4 print"
    elif scope == "stock":
        title = "Faderpunk Stock Apps — UX Cheatsheet"
        sub = f"Stock apps (IDs {lo}–{hi}) · {today} · A4 print"
    else:
        title = "Faderpunk WIP Apps — UX Cheatsheet"
        sub = f"Synced to <code>test/playground</code> (IDs {lo}–{hi}) · {today} · A4 print"

    shell = re.sub(
        r"<title>.*?</title>",
        f"<title>{title}</title>",
        shell,
        count=1,
        flags=re.S,
    )
    shell = re.sub(
        r"<h1>.*?</h1>",
        f"<h1>{title}</h1>",
        shell,
        count=1,
        flags=re.S,
    )
    shell = re.sub(
        r'<p class="sub">.*?</p>',
        f'<p class="sub">{sub}</p>',
        shell,
        count=1,
        flags=re.S,
    )
    return shell


def assemble(scope: str) -> str:
    apps = load_app_docs()
    legacy = load_legacy()

    sections: dict[int, str] = {}
    meta_status: dict[int, str] = {}

    for app_id, html in legacy.items():
        sections[app_id] = html if html.endswith("\n\n") else html.rstrip() + "\n\n"
        meta_status[app_id] = "wip"  # legacy tree is currently WIP-only

    for app_id, doc in apps.items():
        status = doc.get("status", "wip")
        meta_status[app_id] = status
        sections[app_id] = render_app_html(doc)

    if scope in ("wip", "stock"):
        keep = {i for i, st in meta_status.items() if st == scope}
        # Legacy without status field treated as wip above.
        sections = {i: s for i, s in sections.items() if i in keep}
    elif scope != "all":
        raise SystemExit(f"unknown scope: {scope}")

    if not SHELL.exists() or not FOOTER.exists():
        raise SystemExit("missing docs/cheatsheet/shell.html or footer.html")

    ids = sorted(sections)
    shell = patch_shell(SHELL.read_text(), scope, ids)
    body = "".join(sections[i] for i in ids)
    footer = FOOTER.read_text()
    if not shell.endswith("\n"):
        shell += "\n"
    return shell + body + footer


def maybe_pdf() -> None:
    chrome = Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
    if not chrome.exists():
        print("WARN: Chrome not found; skip PDF", file=sys.stderr)
        return
    url = OUT_HTML.resolve().as_uri()
    subprocess.run(
        [
            str(chrome),
            "--headless",
            "--disable-gpu",
            "--no-pdf-header-footer",
            f"--print-to-pdf={OUT_PDF}",
            url,
        ],
        check=False,
        capture_output=True,
    )
    print(f"wrote {OUT_PDF.relative_to(ROOT)}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--scope",
        choices=("wip", "stock", "all"),
        default="wip",
        help="which apps to include (default: wip)",
    )
    ap.add_argument("--pdf", action="store_true", help="also regenerate PDF via Chrome")
    ap.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if committed HTML differs from assembly",
    )
    args = ap.parse_args()

    html = assemble(args.scope)
    if args.check:
        current = OUT_HTML.read_text() if OUT_HTML.exists() else ""
        if current != html:
            print("FAIL: cheatsheet HTML is stale; run scripts/assemble-cheatsheet.py")
            return 1
        print("OK  cheatsheet HTML matches assembly")
        return 0

    OUT_HTML.write_text(html)
    print(f"wrote {OUT_HTML.relative_to(ROOT)} ({html.count('class=\"app\"')} apps)")
    if args.pdf:
        maybe_pdf()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
