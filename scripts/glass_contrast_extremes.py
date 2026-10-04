"""Check Dynamic Glass text and controls over extreme artwork/desktop values."""

import json
import os
import subprocess
import time
import tkinter as tk
from pathlib import Path

from PIL import Image

from interaction import OUT, close, save_test_screenshot, test_monitor_rect, u, wait_window, wait_window_on_monitor


if os.environ.get("ISLE_TEST_MONITOR") != r"\\.\DISPLAY2":
    raise SystemExit("ISLE_TEST_MONITOR must point to DISPLAY2")

exe = Path(os.environ["ISLE_TEST_EXE"]).resolve()
folder = OUT / "glass-contrast-extremes"
folder.mkdir(exist_ok=True)
monitor = test_monitor_rect()
root = tk.Tk()
root.overrideredirect(True)
root.attributes("-topmost", True)
canvas = tk.Canvas(root, width=1920, height=1032, highlightthickness=0)
canvas.pack()
cases = []


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
    raise AssertionError("contrast snapshot timeout")


def rect_crop(image, values, scale):
    x, y, width, height = values
    return image.crop((
        round(x * scale), round(y * scale),
        round((x + width) * scale), round((y + height) * scale),
    ))


def percentile(histogram, fraction):
    target = round(sum(histogram) * fraction)
    cumulative = 0
    for value, count in enumerate(histogram):
        cumulative += count
        if cumulative >= target:
            return value
    return 255


try:
    for dpi in (96, 144, 192):
        for art in ("bright", "dark"):
            for desktop, color in (("white", "#ffffff"), ("black", "#000000")):
                canvas.configure(background=color)
                canvas.delete("all")
                canvas.create_rectangle(0, 0, 1920, 1032, fill=color, outline=color)
                root.geometry(f"1920x1032{monitor.left:+d}{monitor.top:+d}")
                root.update()
                report = folder / f"{dpi}-{art}-{desktop}.json"
                report.unlink(missing_ok=True)
                process = subprocess.Popen([
                    str(exe), "--ui-v2", "--test-fixture", f"--test-cover-{art}",
                    "--page", "music", "--paused", "--reduced-motion", "--benchmark",
                    "--test-dpi", str(dpi), "--settings-path",
                    str(folder / f"{dpi}-{art}-{desktop}-settings.json"), "--log", str(report),
                ])
                hwnd = 0
                try:
                    hwnd = wait_window(process)
                    bounds = wait_window_on_monitor(hwnd, monitor, f"contrast {dpi} DPI")
                    state = snapshot(hwnd, report)
                    if state.get("fontFamily") != "MiSans" or state.get("expanded") is not True:
                        raise AssertionError((dpi, art, desktop, state))
                    time.sleep(0.12)
                    screenshot = folder / f"{dpi}-{art}-{desktop}.png"
                    save_test_screenshot(screenshot, bounds)
                    with Image.open(screenshot) as source:
                        image = source.convert("RGB")
                        scale = state["scale"]
                        surface = state["surfaceRect"]
                        core = image.getpixel((
                            round((surface[0] + surface[2] * 0.5) * scale),
                            round((surface[1] + surface[3] * 0.2) * scale),
                        ))
                        if max(core) > 18:
                            raise AssertionError((dpi, art, desktop, "black core lost contrast", core))

                        title = rect_crop(image, state["titleRect"], scale)
                        text_histogram = title.convert("L").histogram()
                        text_pixels = sum(text_histogram[171:])
                        if text_pixels < 8:
                            raise AssertionError((dpi, art, desktop, "title lacks bright foreground pixels", text_pixels))

                        control_deltas = []
                        for control in state["controlRects"][-3:]:
                            region = rect_crop(image, control, scale)
                            luminance = region.convert("L").histogram()
                            delta = percentile(luminance, 0.95) - percentile(luminance, 0.50)
                            control_deltas.append(delta)
                        if not control_deltas or max(control_deltas) < 18:
                            raise AssertionError((dpi, art, desktop, "controls blend into background", control_deltas))
                        cases.append({"dpi": dpi, "art": art, "desktop": desktop,
                                      "titleForegroundPixels": text_pixels,
                                      "controlContrastDeltas": control_deltas,
                                      "blackCore": core, "screenshot": str(screenshot)})
                        print(f"PASS: DISPLAY2 {dpi} DPI, {art} artwork on {desktop} desktop, text and controls remain distinct")
                finally:
                    if hwnd and process.poll() is None:
                        close(process, hwnd)
                    elif process.poll() is None:
                        raise AssertionError(f"Owned contrast process {process.pid} has no closable HWND")
                assert process.returncode == 0, (dpi, art, desktop, process.returncode)
finally:
    root.destroy()

(folder / "results.json").write_text(json.dumps(cases, indent=2), encoding="utf-8")
