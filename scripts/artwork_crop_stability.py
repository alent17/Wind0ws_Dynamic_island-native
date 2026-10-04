"""Check shared album geometry with landscape and portrait test covers."""

import json
import os
import subprocess
import time
from pathlib import Path

from PIL import Image

from interaction import OUT, close, save_test_screenshot, test_monitor_rect, u, wait_window, wait_window_on_monitor


if os.environ.get("ISLE_TEST_MONITOR") != r"\\.\DISPLAY2":
    raise SystemExit("ISLE_TEST_MONITOR must point to DISPLAY2")

exe = Path(os.environ["ISLE_TEST_EXE"]).resolve()
folder = OUT / "artwork-crop-stability"
folder.mkdir(exist_ok=True)


def snapshot(hwnd, report):
    previous = report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd, 0x803C, 0, 0)
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if report.exists() and report.stat().st_mtime_ns != previous:
            try:
                return json.loads(report.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                pass
        time.sleep(0.01)
    raise AssertionError("diagnostic snapshot timeout")


def settled(hwnd, report, expanded):
    deadline = time.monotonic() + 8
    state = None
    while time.monotonic() < deadline:
        state = snapshot(hwnd, report)
        if state.get("expanded") is expanded and not state.get("continuous"):
            return state
        time.sleep(0.02)
    raise AssertionError(("surface did not settle", expanded, state))


def crop_signature(path, state):
    x, y, width, height = state["albumRect"]
    scale = state["scale"]
    colors = ((30, 80, 220), (235, 90, 40))
    with Image.open(path).convert("RGB") as image:
        signature = []
        for fy in (0.125, 0.375, 0.625, 0.875):
            for fx in (0.375, 0.625):
                point = (round((x + fx * width) * scale), round((y + fy * height) * scale))
                rgb = image.getpixel(point)
                signature.append(min(range(2), key=lambda index: sum(
                    (rgb[channel] - colors[index][channel]) ** 2 for channel in range(3))))
        return signature


monitor = test_monitor_rect()
for dpi in (96, 192):
    for kind, dimensions in (("landscape", (512, 320)), ("portrait", (320, 512))):
        report = folder / f"{dpi}-{kind}.json"
        settings = folder / f"{dpi}-{kind}-settings.json"
        report.unlink(missing_ok=True)
        proc = subprocess.Popen([
            str(exe), "--ui-v2", "--test-fixture", f"--test-cover-{kind}",
            "--paused", "--reduced-motion", "--benchmark", "--test-dpi", str(dpi),
            "--settings-path", str(settings), "--log", str(report),
        ])
        hwnd = 0
        try:
            hwnd = wait_window(proc)
            bounds = wait_window_on_monitor(hwnd, monitor, f"{kind} artwork {dpi} DPI")
            compact = settled(hwnd, report, False)
            (folder / f"{dpi}-{kind}-compact-state.json").write_text(
                json.dumps(compact, indent=2), encoding="utf-8")
            assert compact["artworkSide"] == dimensions[0], compact
            assert compact["artworkHeight"] == dimensions[1], compact
            compact_image = folder / f"{dpi}-{kind}-compact.png"
            save_test_screenshot(compact_image, bounds)

            u.PostMessageW(hwnd, 0x100, 0x0D, 0)
            expanded = settled(hwnd, report, True)
            album = expanded["albumRect"]
            assert album[2] > 100 and album[3] > 100, expanded
            assert abs(album[2] - album[3]) < 0.1, (kind, "album crop must remain square", album)
            surface = expanded["surfaceRect"]
            assert album[0] >= surface[0] and album[1] >= surface[1], (album, surface)
            assert album[0] + album[2] <= surface[0] + surface[2] + 0.1, (album, surface)
            assert album[1] + album[3] <= surface[1] + surface[3] + 0.1, (album, surface)
            expanded_image = folder / f"{dpi}-{kind}-expanded.png"
            save_test_screenshot(expanded_image, bounds)
            compact_signature = crop_signature(compact_image, compact)
            expanded_signature = crop_signature(expanded_image, expanded)
            matches = sum(left == right for left, right in zip(compact_signature, expanded_signature))
            assert matches == 8, (kind, "compact/expanded artwork crop changed", matches, compact_signature, expanded_signature)

            u.PostMessageW(hwnd, 0x100, 0x1B, 0)
            collapsed = settled(hwnd, report, False)
            assert collapsed["artworkHeight"] == dimensions[1], collapsed
            print(f"PASS: DISPLAY2 {dpi} DPI, {kind} {dimensions[0]}x{dimensions[1]}, stable square viewport and 8/8 matching crop samples")
        finally:
            if hwnd and proc.poll() is None:
                close(proc, hwnd)
            elif proc.poll() is None:
                raise AssertionError(f"Owned process {proc.pid} has no closable HWND")
        assert proc.returncode == 0, (kind, dpi, proc.returncode)
