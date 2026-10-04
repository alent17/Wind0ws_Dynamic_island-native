"""DISPLAY2 spring entry, exit and reversal checks for Activity capsules."""
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
folder = OUT / 'activity-motion'
folder.mkdir(exist_ok=True)
config = folder / 'settings.json'
config.write_text('{"enableAnimations":true,"reduceAnimations":false}', encoding='utf-8')
report = folder / 'snapshot.json'
monitor = test_monitor_rect()
proc = subprocess.Popen([
    str(exe), '--ui-v2', '--test-fixture', '--paused',
    '--settings-path', str(config), '--log', str(report),
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
        time.sleep(.01)
    raise AssertionError('Activity motion diagnostic snapshot timed out')


def wait_until(predicate, timeout=4):
    deadline = time.monotonic() + timeout
    state = None
    while time.monotonic() < deadline:
        state = snapshot()
        if predicate(state):
            return state
        time.sleep(.015)
    raise AssertionError(('Activity spring did not settle', state))


def visible_capture(state, label):
    bounds = wait_window_on_monitor(hwnd, monitor, label)
    image_path = folder / f'{label}.png'
    save_test_screenshot(image_path, bounds)
    image = Image.open(image_path).convert('RGB')
    assert state['activityIds'] == ['fixture.timer', 'fixture.volume'], state
    assert len(state['activityRects']) == 2, state
    for x, y, width, height in state['activityRects']:
        left, top = round(x * state['scale']), round(y * state['scale'])
        right = round((x + width) * state['scale'])
        bottom = round((y + height) * state['scale'])
        center = w.POINT(
            bounds.left + (left + right) // 2,
            bounds.top + (top + bottom) // 2,
        )
        assert u.WindowFromPoint(center) == hwnd, (state, center.x, center.y)
        extrema = image.crop((left, top, right, bottom)).getextrema()
        assert all(channel[1] > 80 for channel in extrema), (state, extrema)


try:
    wait_window_on_monitor(hwnd, monitor, 'Activity spring fixture')
    initial = snapshot()
    assert not initial['activityIds'] and not initial['continuous'], initial

    assert u.PostMessageW(hwnd, 0x8040, 2, 0)
    entering = snapshot()
    assert entering['activityIds'] == ['fixture.timer', 'fixture.volume'], entering
    first = entering['activityMotions'][0]
    # Window scheduling can make the diagnostic request arrive a few frames
    # after the posted activity update. Assert it is still entering, without
    # requiring a first-frame opacity sample that is timing-sensitive.
    assert first['targetOpacity'] == 1. and first['opacity'] < 0.85, entering
    assert first['rect'] != first['targetRect'] and entering['continuous'], entering
    time.sleep(.08)
    entering_motion = snapshot()
    assert entering_motion['activityMotions'][0]['rect'] != first['rect'], entering_motion

    assert u.PostMessageW(hwnd, 0x8040, 0, 0)
    exiting = snapshot()
    first_exit = exiting['activityMotions'][0]
    assert first_exit['targetOpacity'] == 0. and first_exit['id'] == 'fixture.timer', exiting
    assert max(abs(a - b) for a, b in zip(
        first_exit['rect'], entering_motion['activityMotions'][0]['rect'])) < 12., exiting
    assert exiting['continuous'], exiting

    time.sleep(.04)
    reversing_from = snapshot()
    assert u.PostMessageW(hwnd, 0x8040, 2, 0)
    reversing = snapshot()
    reversed_slot = reversing['activityMotions'][0]
    assert reversed_slot['targetOpacity'] == 1., reversing
    assert reversed_slot['id'] == 'fixture.timer', reversing
    assert reversed_slot['opacity'] > 0. and reversed_slot['rect'][2] > 0., reversing
    assert reversed_slot['rect'] != reversed_slot['targetRect'] and reversing['continuous'], reversing
    time.sleep(.08)
    reverse_progress = snapshot()
    reverse_slot = reverse_progress['activityMotions'][0]
    old_distance = sum(abs(a - b) for a, b in zip(
        reversed_slot['rect'], reversed_slot['targetRect']))
    new_distance = sum(abs(a - b) for a, b in zip(
        reverse_slot['rect'], reverse_slot['targetRect']))
    assert new_distance < old_distance, reverse_progress

    entered = wait_until(lambda state: not state['continuous']
                         and all(abs(slot['opacity'] - 1.) < .01
                                 for slot in state['activityMotions'][:2]))
    visible_capture(entered, 'entered')

    assert u.PostMessageW(hwnd, 0x8040, 0, 0)
    exited = wait_until(lambda state: not state['continuous']
                        and not state['activityIds']
                        and all(slot['id'] is None for slot in state['activityMotions']))
    assert not exited['activityRects'], exited
    (folder / 'results.json').write_text(json.dumps({
        'monitor': r'\\.\DISPLAY2',
        'initial': initial,
        'entering': entering,
        'exitStarted': exiting,
        'reversed': reversing,
        'settled': entered,
        'exitSettled': exited,
    }, indent=2), encoding='utf-8')
    print('PASS: DISPLAY2 Activity spring enter/exit, velocity-preserving reversal and same-HWND hit testing')
finally:
    close(proc, hwnd)
