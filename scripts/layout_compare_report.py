#!/usr/bin/env python3
import argparse
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run(cmd):
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def resolve_bin(path_str: str) -> Path:
    path = Path(path_str)
    if path.exists():
        return path
    if path_str == "mmdr":
        return path
    return path


def ensure_bin(bin_path: Path):
    if bin_path.exists():
        return
    res = run(["cargo", "build", "--release"])
    if res.returncode != 0:
        raise RuntimeError(res.stderr.strip() or "cargo build failed")


def layout_key(path: Path, base: Path) -> str:
    try:
        rel = path.relative_to(base)
    except ValueError:
        rel = path.name
    rel_no_ext = rel.with_suffix("")
    parts = [part.replace(" ", "_") for part in Path(rel_no_ext).parts]
    return "__".join(parts)


def collect_fixtures(fixtures, limit):
    files = []
    for base in fixtures:
        if base.exists():
            files.extend(sorted(base.glob("**/*.mmd")))
    if limit:
        files = files[:limit]
    return files


def render(bin_path, config_path, inp, out_png):
    cmd = [
        str(bin_path),
        "-i",
        str(inp),
        "-o",
        str(out_png),
        "-e",
        "png",
    ]
    if config_path.exists():
        cmd.extend(["-c", str(config_path)])
    res = run(cmd)
    if res.returncode != 0:
        raise RuntimeError(res.stderr.strip() or "render failed")


def write_report(out_dir: Path, rows, config_path: Path):
    rows_html = []
    for row in rows:
        rows_html.append(
            f"""
        <tr>
            <td>{row['name']}</td>
            <td class="status">{row.get('status', '')}</td>
            <td><img src="{row['before']}" /></td>
            <td><img src="{row['after']}" /></td>
        </tr>
        """
        )
    html = f"""
<!doctype html>
<html>
<head>
<meta charset="utf-8" />
<title>Layout Before vs After (mmdr)</title>
<style>
body {{ font-family: system-ui, -apple-system, sans-serif; margin: 24px; color: #222; }}
img {{ max-width: 360px; height: auto; border: 1px solid #ddd; background: #fff; }}
code {{ background: #f4f4f4; padding: 2px 6px; border-radius: 4px; }}
.table-wrap {{ overflow-x: auto; }}
table {{ border-collapse: collapse; min-width: 900px; }}
th, td {{ border-bottom: 1px solid #eee; padding: 8px 10px; vertical-align: top; text-align: left; }}
th {{ position: sticky; top: 0; background: #fafafa; }}
.muted {{ color: #777; }}
.status {{ max-width: 320px; }}
</style>
</head>
<body>
<h1>Layout Before vs After (mmdr)</h1>
<p>Comparing dagre-based layout (before) vs custom layout (after) across many fixtures.</p>
<p><code>Config</code>: {config_path}</p>
<div class="table-wrap">
<table>
<thead>
<tr>
<th>Fixture</th>
<th>Status</th>
<th>Before (dagre)</th>
<th>After (custom)</th>
</tr>
</thead>
<tbody>
{''.join(rows_html)}
</tbody>
</table>
</div>
</body>
</html>
"""
    report_path = out_dir / "report.html"
    report_path.write_text(html, encoding="utf-8")
    return report_path


def main():
    parser = argparse.ArgumentParser(description="Generate before/after layout report")
    parser.add_argument(
        "--before-bin",
        default=str(ROOT / "target" / "release" / "mmdr-before"),
        help="path to before binary (dagre)",
    )
    parser.add_argument(
        "--after-bin",
        default=str(ROOT / "target" / "release" / "mmdr"),
        help="path to after binary (custom)",
    )
    parser.add_argument(
        "--config",
        default=str(ROOT / "tests" / "fixtures" / "modern-config.json"),
        help="config JSON for mmdr",
    )
    parser.add_argument(
        "--out-dir",
        default=str(ROOT / "docs" / "layout-compare-report"),
        help="output directory",
    )
    parser.add_argument(
        "--fixtures",
        action="append",
        default=[],
        help="fixture dir (repeatable). default: tests/fixtures, benches/fixtures, docs/diagrams",
    )
    parser.add_argument("--limit", type=int, default=0, help="limit number of fixtures")
    args = parser.parse_args()

    fixtures = [Path(p) for p in args.fixtures if p]
    if not fixtures:
        fixtures = [
            ROOT / "tests" / "fixtures",
            ROOT / "benches" / "fixtures",
            ROOT / "docs" / "diagrams",
        ]

    files = collect_fixtures(fixtures, args.limit)
    if not files:
        print("No fixtures found.")
        return 1

    before_bin = resolve_bin(args.before_bin)
    after_bin = resolve_bin(args.after_bin)
    ensure_bin(after_bin)
    if not before_bin.exists():
        raise RuntimeError(f"Before binary missing: {before_bin}")

    config_path = Path(args.config)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    rows = []
    for fixture in files:
        key = layout_key(fixture, ROOT)
        before_png = out_dir / f"{key}-before.png"
        after_png = out_dir / f"{key}-after.png"
        before_raw = out_dir / f"{key}-before-raw.png"
        after_raw = out_dir / f"{key}-after-raw.png"
        status = "ok"
        try:
            render(before_bin, config_path, fixture, before_png)
            render(after_bin, config_path, fixture, after_png)
            before_raw.write_bytes(before_png.read_bytes())
            after_raw.write_bytes(after_png.read_bytes())
        except Exception as exc:
            status = f"error: {exc}"
        rows.append(
            {
                "name": str(fixture),
                "status": status,
                "before": before_png.name,
                "after": after_png.name,
            }
        )

    report_path = write_report(out_dir, rows, config_path)
    print(f"Wrote {report_path} with {len(rows)} rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
