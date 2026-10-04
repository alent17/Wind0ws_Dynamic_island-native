"""Exercise real system volume/mute notifications and their short-lived activity."""
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import time
from interaction import OUT, close, test_monitor_rect, wait_window, wait_window_on_monitor

exe = Path(os.environ['ISLE_TEST_EXE']).resolve()
monitor = test_monitor_rect()
report = OUT / 'volume-activity.json'
proc = subprocess.Popen([
    str(exe), '--live-audio', '--page', 'volume', '--paused',
    '--reduced-motion', '--log', str(report),
])
hwnd = wait_window(proc)
wait_window_on_monitor(hwnd, monitor, 'volume activity window')
u = c.windll.user32
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.keybd_event.argtypes = [w.BYTE, w.BYTE, w.DWORD, c.c_size_t]


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
    raise AssertionError('volume activity diagnostic snapshot timed out')


def wait_snapshot(predicate, message, timeout=4):
    deadline = time.monotonic() + timeout
    latest = None
    while time.monotonic() < deadline:
        latest = snapshot()
        if predicate(latest):
            return latest
        time.sleep(.05)
    raise AssertionError(f'{message}; latest={latest}')


def click_delta(rect, scale, delta):
    x, y, width, height = rect
    pixel_x = round((x + width / 2 + delta * 10) * scale)
    pixel_y = round((y + height / 2) * scale)
    pixel_x = max(round(x * scale) + 1, min(round((x + width) * scale) - 1, pixel_x))
    lp = (pixel_x & 0xFFFF) | ((pixel_y & 0xFFFF) << 16)
    if not u.PostMessageW(hwnd, 0x0201, 1, lp):
        raise c.WinError(c.get_last_error())
    if not u.PostMessageW(hwnd, 0x0202, 0, lp):
        raise c.WinError(c.get_last_error())


def toggle_mute():
    u.keybd_event(0xAD, 0, 0, 0)  # VK_VOLUME_MUTE down
    u.keybd_event(0xAD, 0, 2, 0)  # KEYEVENTF_KEYUP


def restore(baseline_volume, baseline_muted):
    state = snapshot()
    if state.get('audioMuted') != baseline_muted:
        toggle_mute()
        state = wait_snapshot(
            lambda current: current.get('audioMuted') == baseline_muted,
            'could not restore original system mute state',
        )
    if state.get('audioVolume') != baseline_volume:
        rect = state.get('volumeControlRect')
        if not rect:
            raise AssertionError('volume slider geometry missing during restoration')
        delta = baseline_volume - state['audioVolume']
        if abs(delta) >= 19:
            raise AssertionError(f'refusing an unexpectedly large restore adjustment: {delta}')
        click_delta(rect, state['scale'], delta)
        state = wait_snapshot(
            lambda current: current.get('audioVolume') == baseline_volume,
            'could not restore original system volume',
        )
    return state


try:
    initial = wait_snapshot(
        lambda state: state.get('audioDevices', 0) > 0 and state.get('volumeControlRect'),
        'live audio endpoint or volume control did not become ready',
    )
    if initial.get('audioError') or initial.get('volumeActivity') is not None:
        raise AssertionError(f'initial audio snapshot must not emit an activity: {initial}')
    baseline_volume = initial['audioVolume']
    baseline_muted = initial['audioMuted']
    delta = 12 if baseline_volume <= 87 else -12
    click_delta(initial['volumeControlRect'], initial['scale'], delta)
    changed = wait_snapshot(
        lambda state: state.get('audioVolume') != baseline_volume
        and state.get('volumeActivity') is not None,
        'real volume change did not publish its Activity',
    )
    activity = changed['volumeActivity']
    expected_mute_label = '静音' if changed['audioMuted'] else '未静音'
    if (activity['id'] != 'isle.volume'
            or activity['value'] != f"{changed['audioVolume']}% · {expected_mute_label}"
            or abs(activity['progress'] - changed['audioVolume'] / 100) > 0.001
            or activity['expiresAt'] is None
            or activity['completed']):
        raise AssertionError(f'volume Activity did not reflect the observed endpoint: {changed}')

    toggle_mute()
    muted = wait_snapshot(
        lambda state: state.get('audioMuted') != baseline_muted
        and state.get('volumeActivity') is not None,
        'real mute change did not publish its Activity',
    )
    mute_activity = muted['volumeActivity']
    if ('静音' not in mute_activity['value']
            or abs(mute_activity['progress'] - muted['audioVolume'] / 100) > 0.001
            or mute_activity['expiresAt'] <= activity['expiresAt']):
        raise AssertionError(f'mute Activity did not reflect the observed endpoint: {muted}')

    restored = restore(baseline_volume, baseline_muted)
    expiry = restored['volumeActivity']['expiresAt']
    deadline = time.monotonic() + max(0, expiry - restored['elapsedSeconds']) + 1
    expired = restored
    while time.monotonic() < deadline:
        expired = snapshot()
        if expired['elapsedSeconds'] >= expiry and expired['volumeActivity'] is None:
            break
        time.sleep(.05)
    if expired['elapsedSeconds'] < expiry or expired['volumeActivity'] is not None:
        raise AssertionError(f'volume Activity did not expire: {expired}')
    if expired['audioVolume'] != baseline_volume or expired['audioMuted'] != baseline_muted:
        raise AssertionError('system audio state did not remain restored through TTL expiry')
    (OUT / 'volume-activity-results.json').write_text(json.dumps({
        'monitor': r'\\.\DISPLAY2',
        'before': {'volume': baseline_volume, 'muted': baseline_muted},
        'changed': {'volume': changed['audioVolume'], 'activity': activity},
        'muted': {'volume': muted['audioVolume'], 'muted': muted['audioMuted'], 'activity': mute_activity},
        'restored': {'volume': restored['audioVolume'], 'muted': restored['audioMuted']},
        'expired': expired['volumeActivity'] is None,
    }, indent=2), encoding='utf-8')
    print('PASS: DISPLAY2 real volume/mute Activity, actual-state readback, restoration, and TTL expiry')
finally:
    try:
        if 'baseline_volume' in locals() and proc.poll() is None:
            restore(baseline_volume, baseline_muted)
    finally:
        close(proc, hwnd)
