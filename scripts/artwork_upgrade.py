"""Verify that a late 512px cover upgrades the visible 128px Shared Album."""

import json
import os
import subprocess
import time
import tkinter as tk
from pathlib import Path

from interaction import (
    OUT,
    close,
    save_test_screenshot,
    test_monitor_rect,
    u,
    wait_window,
    wait_window_on_monitor,
)


if os.environ.get("ISLE_TEST_MONITOR") != r"\\.\DISPLAY2":
    raise SystemExit("ISLE_TEST_MONITOR must point to DISPLAY2")

exe = Path(os.environ["ISLE_TEST_EXE"])
folder = OUT / "artwork-upgrade"
folder.mkdir(exist_ok=True)
root = tk.Tk()
root.overrideredirect(True)
root.attributes("-topmost", True)
root.geometry("1920x1080-1920+0")
canvas = tk.Canvas(root, width=1920, height=1080, highlightthickness=0)
canvas.pack()
for y in range(0, 1080, 12):
    tint = 48 + (y // 12 % 5) * 22
    canvas.create_rectangle(0, y, 1920, y + 13, fill=f"#{tint:02x}40a0", outline="")
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


def wait_for(hwnd, report, predicate, timeout=5):
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
            str(exe), "--ui-v2", "--test-fixture", "--test-artwork-upgrade",
            "--page", "music", "--paused", "--benchmark", "--test-dpi", str(dpi),
            "--settings-path", str(folder / f"{dpi}-settings.json"), "--log", str(report),
        ])
        hwnd = 0
        try:
            hwnd = wait_window(process)
            bounds = wait_window_on_monitor(hwnd, test_monitor_rect(), f"Artwork {dpi} DPI")
            initial = wait_for(
                hwnd, report,
                lambda state: state.get("expanded") is True
                and state.get("artworkSide") == 128
                and surface_settled(state),
            )
            save_test_screenshot(folder / f"{dpi}-artwork-128.png", bounds)

            u.PostMessageW(hwnd, 0x803E, 0, 0)
            started = wait_for(
                hwnd, report,
                lambda state: state.get("artworkSide") == 512
                and state.get("previousArtworkLayers") == 1,
            )
            assert started["artworkFadeProgress"] < 0.25, started
            settled = wait_for(
                hwnd, report,
                lambda state: state.get("artworkFadeProgress", 0) >= 0.99
                and state.get("previousArtworkLayers") == 0,
            )
            assert same_rect(initial["albumRect"], settled["albumRect"])
            assert settled["renderCpuP95Ms"] < 50, settled
            assert settled["artworkUploadMs"] < 50, settled
            save_test_screenshot(folder / f"{dpi}-artwork-512.png", bounds)
            print(f"PASS: DISPLAY2 {dpi} DPI, expanded on 128px, crossfaded to 512px in place")
        finally:
            if hwnd and process.poll() is None:
                close(process, hwnd)
            elif process.poll() is None:
                raise AssertionError(f"Owned process {process.pid} has no closable HWND")
finally:
    root.destroy()
