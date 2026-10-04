import json
import os
import subprocess
import time
import tkinter as tk
from pathlib import Path

from interaction import OUT, close, save_test_screenshot, u, wait_window, wait_window_on_monitor, test_monitor_rect


if os.environ.get("ISLE_TEST_MONITOR") != r"\\.\DISPLAY2":
    raise SystemExit("ISLE_TEST_MONITOR must point to DISPLAY2")

exe = Path(os.environ["ISLE_TEST_EXE"])
folder = OUT / "shared-album-crossfade"
folder.mkdir(exist_ok=True)
root = tk.Tk()
root.overrideredirect(True)
root.attributes("-topmost", True)
root.geometry("1920x1080-1920+0")
canvas = tk.Canvas(root, width=1920, height=1080, highlightthickness=0)
canvas.pack()
for y in range(0, 1080, 12):
    color = "#%02x%02x%02x" % (24 + y // 12 % 4 * 18, 45 + y // 9 % 5 * 12, 115 + y // 11 % 6 * 17)
    canvas.create_rectangle(0, y, 1920, y + 13, fill=color, outline="")
root.update()


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


def wait_for(hwnd, report, predicate, timeout=4):
    deadline = time.monotonic() + timeout
    state = None
    while time.monotonic() < deadline:
        state = snapshot(hwnd, report)
        if predicate(state):
            return state
        time.sleep(0.01)
    raise AssertionError(("state timeout", state))


def surface_settled(state):
    return all(
        abs(current - target) < 0.1
        for current, target in zip(state["surfaceRect"], state["surfaceTargetRect"])
    )


def same_rect(left, right):
    return all(abs(a - b) < 0.1 for a, b in zip(left, right))


try:
    for dpi in (96, 192):
        report = folder / f"{dpi}-snapshot.json"
        process = subprocess.Popen([
            str(exe), "--ui-v2", "--test-fixture", "--test-cover", "--page", "music",
            "--paused", "--benchmark", "--test-dpi", str(dpi),
            "--settings-path", str(folder / f"{dpi}-settings.json"), "--log", str(report),
        ])
        hwnd = 0
        try:
            hwnd = wait_window(process)
            bounds = wait_window_on_monitor(hwnd, test_monitor_rect(), f"Shared Album {dpi} DPI")
            initial = wait_for(
                hwnd, report,
                lambda state: state.get("expanded") is True and surface_settled(state),
            )
            assert initial["artworkSide"] == 64, initial

            u.PostMessageW(hwnd, 0x803D, 0, 0)
            started = wait_for(
                hwnd, report,
                lambda state: state.get("testArtworkGeneration") == 1
                and state.get("previousArtworkLayers") == 1,
            )
            assert same_rect(started["albumRect"], initial["albumRect"]), ("first track changed album geometry", initial["albumRect"], started["albumRect"])
            assert 0 <= started["artworkFadeProgress"] < 0.25, started
            time.sleep(0.09)
            mid = snapshot(hwnd, report)
            assert 0.05 < mid["artworkFadeProgress"] < 1, mid
            save_test_screenshot(folder / f"{dpi}-mid-crossfade.png", bounds)

            u.PostMessageW(hwnd, 0x803D, 0, 0)
            interrupted = wait_for(
                hwnd, report,
                lambda state: state.get("testArtworkGeneration") == 2
                and state.get("previousArtworkLayers", 0) >= 2,
            )
            assert same_rect(interrupted["albumRect"], started["albumRect"]), ("track change moved album geometry", started["albumRect"], interrupted["albumRect"])
            assert interrupted["previousArtworkLayers"] <= 4, interrupted
            assert interrupted["artworkFadeProgress"] < 0.25, interrupted
            save_test_screenshot(folder / f"{dpi}-interrupted-crossfade.png", bounds)

            settled = wait_for(
                hwnd, report,
                lambda state: state.get("artworkFadeProgress", 0) >= 0.99
                and state.get("previousArtworkLayers") == 0,
            )
            assert settled["surfaceTargetRect"] == initial["surfaceTargetRect"]
            assert same_rect(settled["albumRect"], interrupted["albumRect"]), ("settled album geometry changed", interrupted["albumRect"], settled["albumRect"])
            print(f"PASS: DISPLAY2 {dpi} DPI, shared album rect stable, interrupted crossfade settled")
        finally:
            if hwnd and process.poll() is None:
                close(process, hwnd)
            elif process.poll() is None:
                raise AssertionError(f"Owned process {process.pid} has no closable HWND")
finally:
    root.destroy()
