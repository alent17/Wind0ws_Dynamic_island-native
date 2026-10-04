"""DISPLAY2 screenshot sampling while Dynamic Glass opens and closes."""
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
folder = OUT / 'dynamic-glass-motion'
folder.mkdir(exist_ok=True)
settings = folder / 'settings.json'
settings.write_text('{"enableAnimations":true,"reduceAnimations":false}', encoding='utf-8')
monitor = test_monitor_rect()
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


def snapshot(hwnd, report):
    previous = report.stat().st_mtime_ns if report.exists() else 0
    if not u.PostMessageW(hwnd, 0x803C, 0, 0):
        raise OSError('failed to request fixture snapshot')
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if report.exists() and report.stat().st_mtime_ns != previous:
            try:
                return json.loads(report.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                pass
        time.sleep(.005)
    raise AssertionError('glass motion snapshot timed out')


for dpi in (96, 192):
    report = folder / f'dpi-{dpi}.json'
    report.unlink(missing_ok=True)
    proc = subprocess.Popen([
        str(exe), '--ui-v2', '--test-fixture', '--test-cover', '--paused', '--benchmark',
        '--test-dpi', str(dpi), '--settings-path', str(settings), '--log', str(report),
    ])
    hwnd = None
    try:
        hwnd = wait_window(proc)
        bounds = wait_window_on_monitor(hwnd, monitor, f'Dynamic Glass motion {dpi} DPI')
        state = snapshot(hwnd, report)
        if state['expanded'] or state['surfaceRect'][3] > 40:
            raise AssertionError(('fixture should begin compact', state))

        for direction, key, expanded in (('expand', 0x0D, True), ('collapse', 0x1B, False)):
            u.PostMessageW(hwnd, 0x0100, key, 0)
            deadline = time.monotonic() + 3
            samples = []
            while time.monotonic() < deadline:
                state = snapshot(hwnd, report)
                _, _, _, height = state['surfaceRect']
                if state['continuous'] and height > 60:
                    screenshot = folder / f'{direction}-{dpi}-{len(samples):02d}.png'
                    save_test_screenshot(screenshot, bounds)
                    with Image.open(screenshot) as source:
                        image = source.convert('RGB')
                        x, y, width, live_height = state['surfaceRect']
                        scale = state['scale']
                        black = image.getpixel((round((x + width * .5) * scale),
                                                round((y + live_height * .03) * scale)))
                        if max(black) > 24:
                            raise AssertionError((direction, dpi, 'black core leaked', black, state))
                        clear = None
                        if live_height > 160:
                            clear = image.getpixel((round((x + width * .5) * scale),
                                                    round((y + live_height * .97) * scale)))
                            if max(clear) - min(clear) < 18:
                                raise AssertionError((direction, dpi, 'clear tail lost desktop', clear, state))
                        samples.append({'state': state, 'blackCore': black,
                                        'clearTail': clear, 'screenshot': str(screenshot)})
                if state['expanded'] == expanded and not state['continuous']:
                    break
                time.sleep(.025)
            if state['expanded'] != expanded or state['continuous']:
                raise AssertionError((direction, dpi, 'spring did not settle', state))
            if len(samples) < 2:
                raise AssertionError((direction, dpi, 'insufficient intermediate captures', len(samples)))
            cases.append({'dpi': dpi, 'direction': direction, 'frames': samples,
                          'settled': state})
            print(f'PASS: DISPLAY2 {dpi} DPI Dynamic Glass {direction}, '
                  f'{len(samples)} animated screenshots, black core stable')
    finally:
        if hwnd and proc.poll() is None:
            close(proc, hwnd)
        elif proc.poll() is None:
            raise AssertionError(f'owned glass-motion fixture {proc.pid} has no closable HWND')
    assert proc.returncode == 0, proc.returncode

root.destroy()
(folder / 'results.json').write_text(json.dumps(cases, indent=2), encoding='utf-8')
