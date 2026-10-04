"""Verify up to two compact Activity capsules share the one island window."""
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import time
from PIL import Image
from interaction import (
    OUT, close, save_test_screenshot, test_monitor_rect,
    wait_window, wait_window_on_monitor, u,
)

exe = Path(os.environ['ISLE_TEST_EXE']).resolve()
monitor = test_monitor_rect()
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.WindowFromPoint.argtypes = [w.POINT]
u.WindowFromPoint.restype = w.HWND
results = []


for requested in (0, 1, 2, 3):
    report = OUT / f'activity-slots-{requested}.json'
    proc = subprocess.Popen([
        str(exe), '--test-fixture', '--test-activities', str(requested),
        '--ui-v2', '--paused', '--reduced-motion', '--log', str(report),
    ])
    hwnd = wait_window(proc)
    try:
        bounds = wait_window_on_monitor(hwnd, monitor, f'{requested} Activity slots')

        def snapshot():
            previous = report.stat().st_mtime_ns if report.exists() else 0
            if not u.PostMessageW(hwnd, 0x803C, 0, 0):
                raise c.WinError(c.get_last_error())
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                if report.exists() and report.stat().st_mtime_ns != previous:
                    try:
                        return json.loads(report.read_text(encoding='utf-8'))
                    except json.JSONDecodeError:
                        pass
                time.sleep(.02)
            raise AssertionError('activity-slot diagnostic snapshot timed out')

        state = snapshot()
        expected = min(requested, 2)
        assert state['uiV2'] and not state['expanded'], state
        assert len(state['activityIds']) == expected, state
        assert len(state['activityRects']) == expected, state
        if requested >= 2:
            assert state['activityIds'] == ['fixture.timer', 'fixture.volume'], state

        rect = w.RECT()
        u.GetWindowRect(hwnd, c.byref(rect))
        image_path = OUT / f'activity-slots-{requested}.png'
        save_test_screenshot(image_path, bounds)
        image = Image.open(image_path).convert('RGB')
        slot_samples = []
        for values in state['activityRects']:
            x, y, width, height = values
            left = round(x * state['scale'])
            top = round(y * state['scale'])
            right = round((x + width) * state['scale'])
            bottom = round((y + height) * state['scale'])
            center = w.POINT(
                bounds.left + (left + right) // 2,
                bounds.top + (top + bottom) // 2,
            )
            assert u.WindowFromPoint(center) == hwnd, (
                'activity capsule is outside the shared island input region',
                state,
                center.x,
                center.y,
            )
            crop = image.crop((left, top, right, bottom))
            extrema = crop.getextrema()
            assert all(channel[1] > 80 for channel in extrema), (
                'activity capsule content is not visible in the DISPLAY2 capture',
                state,
                extrema,
            )
            slot_samples.append({'rect': values, 'extrema': extrema})
        results.append({
            'requested': requested,
            'shown': expected,
            'activityIds': state['activityIds'],
            'surfaceRect': state['surfaceRect'],
            'activityRects': state['activityRects'],
            'slotPixelExtrema': slot_samples,
            'screenshot': str(image_path),
        })
    finally:
        close(proc, hwnd)

(OUT / 'activity-slots-results.json').write_text(
    json.dumps({'monitor': r'\\.\DISPLAY2', 'cases': results}, indent=2),
    encoding='utf-8',
)
print('PASS: 0/1/2/3 requested Activities show 0/1/2/2 side capsules on one DISPLAY2 HWND')
