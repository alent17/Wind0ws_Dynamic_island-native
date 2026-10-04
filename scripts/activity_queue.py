"""Verify priority ordering and hidden/visible TTL expiry on DISPLAY2."""
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

if os.environ.get('ISLE_TEST_MONITOR') != r'\\.\DISPLAY2':
    raise SystemExit('ISLE_TEST_MONITOR must point to DISPLAY2')

exe = Path(os.environ['ISLE_TEST_EXE']).resolve()
monitor = test_monitor_rect()
report = OUT / 'activity-queue.json'
proc = subprocess.Popen([
    str(exe), '--test-fixture', '--test-activity-queue', '--ui-v2',
    '--paused', '--reduced-motion', '--log', str(report),
])
hwnd = wait_window(proc)
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.WindowFromPoint.argtypes = [w.POINT]
u.WindowFromPoint.restype = w.HWND


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
    raise AssertionError('Activity queue diagnostic snapshot timed out')


def wait_until(predicate, timeout, label):
    deadline = time.monotonic() + timeout
    latest = None
    while time.monotonic() < deadline:
        latest = snapshot()
        if predicate(latest):
            return latest
        time.sleep(.08)
    raise AssertionError((label, latest))


def verify_visible_capsules(state, label):
    assert state['uiV2'] and not state['expanded'], state
    assert len(state['activityRects']) == len(state['activityIds']) == 2, state
    bounds = wait_window_on_monitor(hwnd, monitor, label)
    image_path = OUT / f'activity-queue-{label}.png'
    save_test_screenshot(image_path, bounds)
    image = Image.open(image_path).convert('RGB')
    for x, y, width, height in state['activityRects']:
        left, top = round(x * state['scale']), round(y * state['scale'])
        right = round((x + width) * state['scale'])
        bottom = round((y + height) * state['scale'])
        center = w.POINT(
            bounds.left + (left + right) // 2,
            bounds.top + (top + bottom) // 2,
        )
        assert u.WindowFromPoint(center) == hwnd, (label, state, center.x, center.y)
        extrema = image.crop((left, top, right, bottom)).getextrema()
        assert all(channel[1] > 80 for channel in extrema), (label, state, extrema)


try:
    bounds = wait_window_on_monitor(hwnd, monitor, 'Activity queue')
    initial = snapshot()
    assert initial['activityIds'] == ['queue.primary', 'queue.secondary'], initial
    assert initial['activityQueued'] == 2, initial
    assert initial['activityNextExpiry'] is not None, initial
    verify_visible_capsules(initial, 'initial')

    hidden_expired = wait_until(
        lambda state: state['activityQueued'] == 1
        and state['activityIds'] == ['queue.primary', 'queue.secondary']
        and state['activityNextExpiry'] is not None
        and state['activityNextExpiry'] > 5.,
        5,
        'hidden lower-priority TTL expiry',
    )
    verify_visible_capsules(hidden_expired, 'hidden-expired')

    promoted = wait_until(
        lambda state: state['activityIds'] == ['queue.secondary', 'queue.fallback']
        and state['activityQueued'] == 0
        and state['activityNextExpiry'] is None,
        5,
        'visible TTL expiry and queue promotion',
    )
    verify_visible_capsules(promoted, 'promoted')
    (OUT / 'activity-queue-results.json').write_text(json.dumps({
        'monitor': r'\\.\DISPLAY2',
        'initial': initial,
        'afterHiddenExpiry': hidden_expired,
        'afterVisibleExpiry': promoted,
    }, indent=2), encoding='utf-8')
    print('PASS: DISPLAY2 priority arbitration, hidden TTL cleanup and visible-slot promotion use one HWND')
finally:
    close(proc, hwnd)
