#!/usr/bin/env python3
"""Capture actual app frames (Windows or X11). Requires a desktop/display."""
import argparse
import itertools
import os
from pathlib import Path
import struct
import subprocess
import time

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=Path("ui-captures"))
    parser.add_argument("--quick", action="store_true", help="Only 6 representative screenshots")
    parser.add_argument("--strict-pixels", action="store_true", help="Require exact screenshot dimensions (100% native OS scale)")
    args = parser.parse_args()
    exe = args.binary.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    fixtures = out / "fixtures"
    subprocess.run([str(exe), "--ui-fixtures", str(fixtures)], check=True, timeout=120)
    sizes = [(1280,720),(1920,1080),(2560,1600)]
    cases = list(itertools.product(sizes,[1.0,1.5,2.0],[10,100,1000],["light","dark"]))
    if args.quick:
        cases = [(size,scale,count,theme) for size,scale,count in zip(sizes,[1.0,1.5,2.0],[10,100,1000]) for theme in ("light","dark")]
    report = []
    for (w,h),scale,count,theme in cases:
        name = f"{w}x{h}_{int(scale*100)}_{count}_{theme}"
        dest = out / (name + ".png")
        dest.unlink(missing_ok=True)
        with (out / (name + ".log")).open("wb") as log:
            p = subprocess.Popen([str(exe), str(fixtures / f"{count}.morph"),
                "--screenshot",str(dest),"--ui-size",f"{w}x{h}","--ui-scale",str(scale),"--ui-theme",theme],
                stdout=log,stderr=subprocess.STDOUT,env=os.environ.copy())
            try:
                deadline = time.monotonic() + 60
                while time.monotonic() < deadline:
                    if dest.exists():
                        data = dest.read_bytes()
                        if data.startswith(b"\x89PNG\r\n\x1a\n") and data.endswith(b"IEND\xaeB`\x82"):
                            actual = struct.unpack(">II", data[16:24])
                            if args.strict_pixels and actual != (w,h):
                                raise RuntimeError(f"{name}: expected {(w,h)}, got {actual}")
                            report.append(f"{name}: {actual[0]}x{actual[1]}")
                            print(report[-1], flush=True)
                            break
                    if p.poll() is not None:
                        raise RuntimeError(f"{name}: app exited {p.returncode}; see log")
                    time.sleep(0.2)
                else:
                    raise RuntimeError(f"{name}: no complete PNG within 60s; see log")
            finally:
                if p.poll() is None:
                    p.terminate()
                    try:
                        p.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        p.kill()
                        p.wait()
        (out / "capture-report.txt").write_text("\n".join(report)+"\n", encoding="utf-8")

if __name__ == "__main__":
    main()
