"""Isolated HWND V2 motion/DPI/seek/idle checks; no system media commands.

Synthetic DPI and posted messages do not substitute for visual or hardware QA.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import math
import os
import subprocess
import time
from pathlib import Path
from interaction import ROOT, OUT, wait_window, close, u

exe = Path(os.environ.get('ISLE_TEST_EXE', ROOT / 'target/release/isle-native.exe')).resolve()
folder = OUT / 'ui-v2-motion'
folder.mkdir(exist_ok=True)
config = folder / 'settings.json'
config.write_text('{"enableAnimations":true,"reduceAnimations":false}', encoding='utf-8')
cases = []


def snapshot(hwnd, path):
    before = path.stat().st_mtime_ns if path.exists() else 0
    u.PostMessageW(hwnd, 0x803c, 0, 0)
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if path.exists() and path.stat().st_mtime_ns != before:
            try:
                return json.loads(path.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                pass
        time.sleep(.01)
    raise AssertionError('snapshot timeout')


def settled(hwnd, path, expanded):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        state = snapshot(hwnd, path)
        if state['expanded'] == expanded and not state['continuous']:
            return state
        time.sleep(.02)
    raise AssertionError(('motion did not settle', state))


def launch(name, *extra):
    report = folder / (name + '.json')
    proc = subprocess.Popen([str(exe), '--ui-v2', '--test-fixture', '--test-cover',
                             '--paused', '--benchmark', '--settings-path', str(config),
                             '--log', str(report), *extra])
    return proc, wait_window(proc), report


def key(hwnd, code):
    u.PostMessageW(hwnd, 0x100, code, 0)


def finite_bounds(state):
    for field in ['albumRect', 'surfaceRect', 'surfaceTargetRect', 'progressRect']:
        assert all(math.isfinite(v) for v in state[field]), (field, state)
    assert state['surfaceRect'][2] > 0 and state['surfaceRect'][3] > 0, state


for dpi in [96, 120, 144, 168, 192]:
    for edge in ['top', 'right', 'bottom', 'left']:
        proc, hwnd, report = launch(f'dpi-{dpi}-{edge}', '--attached', '--edge', edge,
                                    '--test-dpi', str(dpi), '--reduced-motion', '--page', 'music')
        try:
            state = settled(hwnd, report, True)
            finite_bounds(state)
            assert abs(state['scale'] - dpi / 96) < .001, state
            rect = w.RECT()
            u.GetWindowRect(hwnd, c.byref(rect))
            assert rect.right - rect.left == round(480 * dpi / 96), state
            album = state['albumRect']
            assert abs(album[2] - album[3]) < .01 and album[2] >= 100, state
            # Same geometry drives the native pointer seek and diagnostics.
            x, y, width, height = state['progressRect']
            packed = (round((x + width * .75) * state['scale']) & 65535) | (
                (round((y + height / 2) * state['scale']) & 65535) << 16)
            u.PostMessageW(hwnd, 0x201, 1, packed)
            u.PostMessageW(hwnd, 0x202, 0, packed)
            state = snapshot(hwnd, report)
            deadline = time.monotonic() + 2
            while abs(state['mediaPositionMillis'] / state['mediaDurationMillis'] - .75) > .03:
                assert time.monotonic() < deadline, state
                state = snapshot(hwnd, report)
            cases.append({'case': 'synthetic-dpi-four-edge-shared-album-seek', 'dpi': dpi,
                          'edge': edge, 'snapshot': state})
        finally:
            close(proc, hwnd)
        assert proc.returncode == 0, proc.returncode

proc, hwnd, report = launch('stress-idle')
try:
    state = settled(hwnd, report, False)
    # Actual motion may snap if Windows requests reduced motion; record this.
    for iteration in range(30):
        key(hwnd, 0x0d)
        state = settled(hwnd, report, True)
        finite_bounds(state)
        assert state['albumRect'][2] > 100, state
        key(hwnd, 0x1b)
        state = settled(hwnd, report, False)
        finite_bounds(state)
        assert state['albumRect'][2] < 25, state
    for delay in [.02, .05, .08]:
        for _ in range(4):
            key(hwnd, 0x0d)
            time.sleep(delay)
            finite_bounds(snapshot(hwnd, report))
            key(hwnd, 0x1b)
            settled(hwnd, report, False)
    state = settled(hwnd, report, False)
    x, y, width, height = state['surfaceRect']
    packed = (round((x + width / 2) * state['scale']) & 65535) | (
        (round((y + height / 2) * state['scale']) & 65535) << 16)
    u.PostMessageW(hwnd, 0x200, 0, packed)
    time.sleep(.04)
    assert snapshot(hwnd, report)['hovered']
    u.PostMessageW(hwnd, 0x2a3, 0, 0)
    u.PostMessageW(hwnd, 0x201, 1, packed)
    # Cancel during the grace interval must retain a future expiry timer.
    u.PostMessageW(hwnd, 0x1f, 0, 0)
    time.sleep(.25)
    assert not snapshot(hwnd, report)['hovered']
    idle_before = settled(hwnd, report, False)
    time.sleep(2)
    idle_after = snapshot(hwnd, report)
    assert idle_after['frames'] == idle_before['frames'], (idle_before, idle_after)
    assert idle_after['regionUpdates'] == idle_before['regionUpdates'], (idle_before, idle_after)
    assert idle_after['timerIntervalMs'] == 0, idle_after
    cases.append({'case': '30-expand-collapse-rapid-reverse-and-idle',
                  'before': idle_before, 'after': idle_after})
finally:
    close(proc, hwnd)
assert proc.returncode == 0, proc.returncode

(folder / 'results.json').write_text(json.dumps({
    'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(), 'cases': cases,
    'screenshotsVerified': False, 'realHardwareDpiVerified': False,
    'reversalPercentages': 'Exact 20/50/80 percent verified by pure Spring tests; HWND uses time samples.',
}, indent=2), encoding='utf-8')
print('PASS: V2 synthetic 5 DPI x 4 edges, shared album, seek, 30 cycles, reverse smoke and idle')
