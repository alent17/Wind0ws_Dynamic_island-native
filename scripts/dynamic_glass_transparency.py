"""DISPLAY2 transparency and solid-core regression for Dynamic Glass.

Each run owns a fixture process and a patterned desktop window. The test samples
the rendered client image to prove the core stays black while the clear tail
reveals saturated desktop color at native and synthetic high DPI.
"""
import json
import os
from pathlib import Path
import subprocess
import time
import tkinter as tk

from PIL import Image
from interaction import OUT, close, save_test_screenshot, test_monitor_rect, u, wait_window, wait_window_on_monitor

if os.environ.get('ISLE_TEST_MONITOR') != r'\\.\DISPLAY2':
    raise SystemExit('ISLE_TEST_MONITOR must point to DISPLAY2')

exe = Path(os.environ['ISLE_TEST_EXE']).resolve()
settings = OUT / 'dynamic-glass-settings.json'
report = OUT / 'dynamic-glass-snapshot.json'
root = tk.Tk()
root.overrideredirect(True)
root.attributes('-topmost', True)
root.geometry('1920x1032-1920+0')
canvas = tk.Canvas(root, width=1920, height=1032, highlightthickness=0)
canvas.pack()
for y in range(0, 1032, 8):
    rgb = tuple(int(start + (end - start) * y / 1032)
                for start, end in zip((30, 20, 235), (255, 40, 120)))
    canvas.create_rectangle(0, y, 1920, y + 9,
                            fill='#%02x%02x%02x' % rgb, outline='')
for x, tint in ((100, '#fff000'), (700, '#20ff88'), (1300, '#13d9ff')):
    canvas.create_oval(x, 200, x + 160, 360, fill=tint, outline='')
root.update()
cases = []

try:
    for dpi in (96, 192):
        settings.write_text('{"enableAnimations":true,"reduceAnimations":true}', encoding='utf-8')
        report.unlink(missing_ok=True)
        proc = subprocess.Popen([
            str(exe), '--ui-v2', '--test-fixture', '--page', 'music', '--paused',
            '--reduced-motion', '--benchmark', '--test-dpi', str(dpi),
            '--settings-path', str(settings), '--log', str(report),
        ])
        hwnd = None
        try:
            hwnd = wait_window(proc)
            bounds = wait_window_on_monitor(hwnd, test_monitor_rect(), f'Dynamic Glass {dpi} DPI')
            previous_mtime = report.stat().st_mtime_ns if report.exists() else 0
            if not u.PostMessageW(hwnd, 0x803C, 0, 0):
                raise OSError('failed to request renderer diagnostics')
            deadline = time.monotonic() + 3
            state = None
            while time.monotonic() < deadline:
                if report.exists() and report.stat().st_mtime_ns != previous_mtime:
                    try:
                        state = json.loads(report.read_text(encoding='utf-8'))
                        break
                    except json.JSONDecodeError:
                        pass
                time.sleep(.01)
            if (state is None or state.get('refractionCaptureReady') is not True
                    or state.get('fontFamily') != 'MiSans'):
                raise AssertionError((dpi, state))
            time.sleep(.2)
            screenshot = OUT / f'dynamic-glass-{dpi}dpi.png'
            save_test_screenshot(screenshot, bounds)
            with Image.open(screenshot) as source:
                image = source.convert('RGB')
                x, y, width, height = state['surfaceRect']
                scale = state['scale']

                def sample(x_fraction, y_fraction):
                    return image.getpixel((round((x + x_fraction * width) * scale),
                                           round((y + y_fraction * height) * scale)))

                black_samples = [sample(x, y) for x, y in (
                    (.12, .12), (.88, .12), (.12, .25), (.88, .25),
                )]
                if any(max(pixel) > 18 for pixel in black_samples):
                    raise AssertionError((dpi, 'solid black core leaked desktop/refraction', black_samples))
                clear_sample = sample(.5, .97)
                if max(clear_sample) - min(clear_sample) < 24:
                    raise AssertionError((dpi, 'transparent tail does not reveal saturated desktop', clear_sample))
                edge_sample = sample(.12, .84)
                cases.append({
                    'dpi': dpi,
                    'fontFamily': state['fontFamily'],
                    'refractionCaptureReady': state['refractionCaptureReady'],
                    'blackSamples': black_samples,
                    'transparentTailSample': clear_sample,
                    'transparentEdgeSample': edge_sample,
                    'screenshot': str(screenshot),
                })
                print(f'PASS: DISPLAY2 {dpi} DPI; pure-black core and transparent desktop verified')
        finally:
            if hwnd and proc.poll() is None:
                close(proc, hwnd)
            elif proc.poll() is None:
                raise AssertionError(f'owned glass fixture {proc.pid} has no closable HWND')
        assert proc.returncode == 0, proc.returncode
finally:
    root.destroy()

(OUT / 'dynamic-glass-results.json').write_text(json.dumps(cases, indent=2), encoding='utf-8')
