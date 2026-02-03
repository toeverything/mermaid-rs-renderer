#!/usr/bin/env python3
from __future__ import annotations

import argparse
import html as html_lib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run_git_log(limit: int) -> list[str]:
    cmd = [
        "git",
        "-C",
        str(ROOT),
        "log",
        f"-n{limit}",
        "--date=short",
        "--pretty=format:%ad %h %s",
    ]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if res.returncode != 0:
        raise RuntimeError(res.stderr.strip() or "git log failed")
    lines = [line.strip() for line in res.stdout.splitlines() if line.strip()]
    return lines


def update_timestamp(html_text: str, now: datetime) -> str:
    iso = now.strftime("%Y-%m-%dT%H:%M:%SZ")
    display = now.strftime("%Y-%m-%d %H:%M UTC")
    attr_pattern = r'(id="regen-ts" data-regen=")[^"]*(")'
    text_pattern = r'(<span id="regen-ts" data-regen="[^"]*">)[^<]*(</span>)'
    if re.search(attr_pattern, html_text) is None or re.search(text_pattern, html_text) is None:
        raise RuntimeError("Failed to update regen timestamp (span not found).")
    updated = re.sub(attr_pattern, rf"\g<1>{iso}\g<2>", html_text)
    updated = re.sub(text_pattern, rf"\g<1>{display}\g<2>", updated)
    return updated


def update_changelog(html_text: str, entries: list[str]) -> str:
    items = "\n".join(f"<li>{html_lib.escape(line)}</li>" for line in entries)
    pattern = r'(<ul class="changelog"[^>]*>)(.*?)(</ul>)'
    if re.search(pattern, html_text, flags=re.S) is None:
        raise RuntimeError("Failed to update changelog list (ul not found).")
    updated = re.sub(pattern, rf"\g<1>\n{items}\n\g<3>", html_text, flags=re.S)
    return updated


def main() -> int:
    parser = argparse.ArgumentParser(description="Update comparison HTML metadata.")
    parser.add_argument(
        "--html",
        default=str(ROOT / "docs" / "comparison.html"),
        help="Path to comparison HTML file.",
    )
    parser.add_argument(
        "--count",
        type=int,
        default=10,
        help="Number of git log entries to include in the changelog.",
    )
    args = parser.parse_args()

    html_path = Path(args.html)
    if not html_path.exists():
        raise SystemExit(f"Comparison HTML not found: {html_path}")

    entries = run_git_log(args.count)
    now = datetime.now(timezone.utc)

    html_text = html_path.read_text()
    html_text = update_timestamp(html_text, now)
    html_text = update_changelog(html_text, entries)
    html_path.write_text(html_text)
    print(f"Updated {html_path} with {len(entries)} changelog entries.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
