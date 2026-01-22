#!/usr/bin/env python3
import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image, ImageChops, ImageStat
except ImportError:  # pragma: no cover
    print("Pillow is required: pip install pillow", file=sys.stderr)
    sys.exit(2)

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FIXTURES = ROOT / "tests" / "fixtures"
DEFAULT_CONFIG = DEFAULT_FIXTURES / "modern-config.json"


def run(cmd):
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def render_rust(input_path: Path, output_path: Path):
    bin_path = ROOT / "target" / "release" / "mermaid-rs-renderer"
    if not bin_path.exists():
        print("Building release binary...", file=sys.stderr)
        res = run(["cargo", "build", "--release"])
        if res.returncode != 0:
            print(res.stderr, file=sys.stderr)
            raise RuntimeError("cargo build failed")
    res = run([str(bin_path), "-i", str(input_path), "-o", str(output_path), "-e", "png"])
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        raise RuntimeError("Rust render failed")


def render_mmdc(input_path: Path, output_path: Path, config_path: Path | None):
    cmd = ["npx", "-y", "@mermaid-js/mermaid-cli", "-i", str(input_path), "-o", str(output_path)]
    if config_path and config_path.exists():
        cmd.extend(["-c", str(config_path)])

    puppeteer_cfg = os.environ.get("MMD_PUPPETEER_CONFIG")
    if puppeteer_cfg:
        cmd.extend(["-p", puppeteer_cfg])

    res = run(cmd)
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        raise RuntimeError("mmdc render failed")


def pad_to_max(img_a: Image.Image, img_b: Image.Image):
    max_w = max(img_a.width, img_b.width)
    max_h = max(img_a.height, img_b.height)
    bg = (255, 255, 255, 255)

    def pad(img):
        if img.mode != "RGBA":
            img = img.convert("RGBA")
        canvas = Image.new("RGBA", (max_w, max_h), bg)
        canvas.paste(img, (0, 0))
        return canvas

    return pad(img_a), pad(img_b)


def diff_images(path_a: Path, path_b: Path):
    img_a = Image.open(path_a)
    img_b = Image.open(path_b)
    img_a, img_b = pad_to_max(img_a, img_b)
    diff = ImageChops.difference(img_a, img_b)
    stat = ImageStat.Stat(diff)
    mean = sum(stat.mean) / len(stat.mean)
    rms = sum(stat.rms) / len(stat.rms)
    return mean, rms


def main():
    parser = argparse.ArgumentParser(description="Compare Rust renderer output vs mermaid-cli")
    parser.add_argument("--input", type=str, default=str(DEFAULT_FIXTURES), help=".mmd file or directory")
    parser.add_argument("--config", type=str, default=str(DEFAULT_CONFIG), help="mmdc config file")
    parser.add_argument("--strict", action="store_true", help="Fail if diff exceeds thresholds")
    parser.add_argument("--mean-threshold", type=float, default=5.0, help="Mean diff threshold")
    parser.add_argument("--rms-threshold", type=float, default=8.0, help="RMS diff threshold")
    args = parser.parse_args()

    input_path = Path(args.input)
    if input_path.is_dir():
        files = sorted(input_path.glob("**/*.mmd"))
    else:
        files = [input_path]

    if not files:
        print("No .mmd fixtures found.", file=sys.stderr)
        return 1

    failed = False
    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir)
        for file in files:
            rust_out = tmpdir / f"{file.stem}-rust.png"
            mmdc_out = tmpdir / f"{file.stem}-mmdc.png"
            print(f"Comparing {file}...")
            render_rust(file, rust_out)
            render_mmdc(file, mmdc_out, Path(args.config))
            mean, rms = diff_images(rust_out, mmdc_out)
            print(f"  mean diff: {mean:.2f}, rms diff: {rms:.2f}")
            if args.strict and (mean > args.mean_threshold or rms > args.rms_threshold):
                failed = True

    if failed:
        print("One or more comparisons exceeded thresholds.", file=sys.stderr)
        return 2

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
