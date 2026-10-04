"""DISPLAY2 leave-grace and pointer-down interaction-envelope regression.

Uses an isolated test fixture and posted messages only. It does not send media
commands or interact with the user's running application.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from interaction import OUT, close, test_monitor_rect, u, wait_window, wait_window_on_monitor

if os.environ.get('ISLE_TEST_MONITOR') != r'\\.\DISPLAY2':
    raise SystemExit('ISLE_TEST_MONITOR must point to DISPLAY2')

exe = Path(os.environ['ISLE_TEST_EXE']).resolve()
folder = OUT / 'ui-v2-interaction-envelope'
folder.mkdir(exist_ok=True)
config = folder / 'settings.json'
config.write_text('{"enableAnimations":true,"reduceAnimations":true}', encoding='utf-8')
monitor = test_monitor_rect()
cases = []


def snapshot(hwnd, report):
    previous = report.stat().st_mtime_ns if report.exists() else 0
    if not u.PostMessageW(hwnd, 0x803C, 0, 0):
        raise OSError('failed to request fixture diagnostic snapshot')
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if report.exists() and report.stat().st_mtime_ns != previous:
            try:
                return json.loads(report.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                pass
        time.sleep(.01)
    raise AssertionError('diagnostic snapshot timed out')


def wait_for(hwnd, report, predicate, timeout=2):
    deadline = time.monotonic() + timeout
    state = None
    while time.monotonic() < deadline:
        state = snapshot(hwnd, report)
        if predicate(state):
            return state
        time.sleep(.01)
    raise AssertionError(('fixture state timed out', state))


for dpi in (96, 192):
    report = folder / f'dpi-{dpi}.json'
    try:
        report.unlink()
    except FileNotFoundError:
        pass
    proc = subprocess.Popen([
        str(exe), '--ui-v2', '--test-fixture', '--test-cover',
        '--paused', '--benchmark', '--test-dpi', str(dpi), '--settings-path', str(config),
        '--log', str(report),
    ])
    hwnd = None
    try:
        hwnd = wait_window(proc)
        wait_window_on_monitor(hwnd, monitor, f'UI V2 {dpi} DPI fixture')
        initial = wait_for(hwnd, report, lambda state: not state['expanded'] and not state['continuous'])
        scale = initial['scale']
        x, y, width, height = initial['surfaceRect']
        packed = (round((x + width / 2) * scale) & 0xFFFF) | (
            (round((y + height / 2) * scale) & 0xFFFF) << 16)

        # Enter the hit area, leave, and re-enter within the 150ms grace.
        u.PostMessageW(hwnd, 0x803F, 0, packed)
        wait_for(hwnd, report, lambda state: state['hovered'])
        u.PostMessageW(hwnd, 0x02A3, 0, 0)
        grace = wait_for(hwnd, report, lambda state: state['hoverGraceActive'])
        assert not grace['expanded'] and grace['hovered'], grace
        time.sleep(.06)
        reentry_started = time.monotonic()
        u.SendMessageW(hwnd, 0x803F, 0, packed)
        reentered = wait_for(hwnd, report, lambda state: not state['hoverGraceActive'])
        reentry_elapsed = time.monotonic() - reentry_started
        assert reentry_elapsed < .12, (reentry_elapsed, reentered)
        assert not reentered['expanded'], reentered

        # A fresh leave expires after the 150ms deadline and clears hover.
        leave_started = time.monotonic()
        u.PostMessageW(hwnd, 0x02A3, 0, 0)
        wait_for(hwnd, report, lambda state: state['hoverGraceActive'])
        expired = wait_for(hwnd, report,
                           lambda state: not state['hoverGraceActive'] and not state['hovered'],
                           timeout=1)
        grace_duration = time.monotonic() - leave_started
        assert .12 <= grace_duration <= .5, (grace_duration, expired)
        assert not expired['expanded'], expired

        # A held pointer protects the surface beyond the leave deadline.
        u.PostMessageW(hwnd, 0x803F, 0, packed)
        wait_for(hwnd, report, lambda state: state['hovered'])
        u.PostMessageW(hwnd, 0x0201, 1, packed)
        pressed = wait_for(hwnd, report, lambda state: state['pointerDown'])
        u.PostMessageW(hwnd, 0x02A3, 0, 0)
        wait_for(hwnd, report, lambda state: state['hoverGraceActive'])
        time.sleep(.22)
        locked = snapshot(hwnd, report)
        assert locked['pointerDown'] and locked['hovered'] and not locked['expanded'], locked
        u.PostMessageW(hwnd, 0x0202, 0, packed)
        released = wait_for(hwnd, report, lambda state: not state['pointerDown'])
        final = wait_for(hwnd, report,
                         lambda state: not state['hoverGraceActive'] and not state['hovered'])
        cases.append({'dpi': dpi, 'initial': initial, 'grace': grace,
                      'reentered': reentered, 'reentryMs': round(reentry_elapsed * 1000),
                      'expired': expired, 'graceMs': round(grace_duration * 1000), 'pressed': pressed,
                      'lockedBeyondGrace': locked, 'released': released, 'final': final})
        print(f'PASS: DISPLAY2 {dpi} DPI leave grace, re-entry cancellation and pointer-down lock')
    finally:
        if hwnd and proc.poll() is None:
            close(proc, hwnd)
        elif proc.poll() is None:
            raise AssertionError(f'owned fixture {proc.pid} has no closable HWND')
    assert proc.returncode == 0, proc.returncode

(folder / 'results.json').write_text(json.dumps({
    'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
    'cases': cases,
    'monitor': r'\\.\DISPLAY2',
}, indent=2), encoding='utf-8')
