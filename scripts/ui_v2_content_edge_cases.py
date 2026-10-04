"""DISPLAY2 visual regressions for long media text and empty artwork states."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from interaction import (
    OUT, close, save_test_screenshot, test_monitor_rect, u, wait_window,
    wait_window_on_monitor, w,
)

if os.environ.get('ISLE_TEST_MONITOR') != r'\\.\DISPLAY2':
    raise SystemExit('ISLE_TEST_MONITOR must point to DISPLAY2')

exe = Path(os.environ['ISLE_TEST_EXE']).resolve()
folder = OUT / 'ui-v2-content-edge-cases'
folder.mkdir(exist_ok=True)
config = folder / 'settings.json'
config.write_text('{"enableAnimations":true,"reduceAnimations":true}', encoding='utf-8')
monitor = test_monitor_rect()
cases = []


def snapshot(hwnd, report):
    before = report.stat().st_mtime_ns if report.exists() else 0
    if not u.PostMessageW(hwnd, 0x803C, 0, 0):
        raise OSError('failed to request fixture snapshot')
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if report.exists() and report.stat().st_mtime_ns != before:
            try:
                return json.loads(report.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                pass
        time.sleep(.01)
    raise AssertionError('fixture snapshot timed out')


def wait_for(hwnd, report, predicate):
    deadline = time.monotonic() + 5
    state = None
    while time.monotonic() < deadline:
        state = snapshot(hwnd, report)
        if predicate(state):
            return state
        time.sleep(.02)
    raise AssertionError(('content fixture did not reach expected state', state))


def inside(surface, child):
    sx, sy, sw, sh = surface
    x, y, width, height = child
    return (width > 0 and height > 0 and x >= sx - .1 and y >= sy - .1
            and x + width <= sx + sw + .1 and y + height <= sy + sh + .1)


def visible(rect):
    return rect[2] > 0 and rect[3] > 0


for dpi in (96, 144, 192):
    dpi_cases = []
    for name, args in (
        ('reference-music', ['--test-cover']),
        ('long-text', ['--test-cover', '--test-long-text']),
        ('no-media', []),
        ('missing-artwork', ['--test-missing-artwork']),
    ):
        config.write_text(json.dumps({
            'enableAnimations': True, 'reduceAnimations': True,
            **({key: False for key in (
                'showTimerTool', 'showVolumeTool', 'showWeatherTool',
                'showClockTool', 'showFloatingTool', 'showSettingsTool',
                'showHideTool',
            )} if name == 'reference-music' else {}),
        }), encoding='utf-8')
        report = folder / f'{name}-{dpi}.json'
        report.unlink(missing_ok=True)
        proc = subprocess.Popen([
            str(exe), '--ui-v2', '--test-fixture', '--paused', '--benchmark',
            '--page', 'music', '--reduced-motion', '--test-dpi', str(dpi),
            '--settings-path', str(config), '--log', str(report), *args,
        ])
        hwnd = None
        try:
            hwnd = wait_window(proc)
            bounds = wait_window_on_monitor(hwnd, monitor, f'{name} DISPLAY2 {dpi} DPI')
            state = wait_for(
                hwnd,
                report,
                lambda value: value['expanded'] and not value['continuous']
                and (name != 'long-text' or value['titleOverflow'])
                and (name == 'no-media'
                     or sum(visible(rect) for rect in value['controlRects']) >= 4),
            )
            assert state['uiV2'] and state['fontFamily'] == 'MiSans', state
            assert inside(state['surfaceRect'], state['titleRect']), state
            assert inside(state['surfaceRect'], state['artistRect']), state
            visible_controls = [rect for rect in state['controlRects'] if visible(rect)]
            assert all(inside(state['surfaceRect'], rect) for rect in visible_controls), state
            if name in ('reference-music', 'long-text'):
                assert state['mediaSeekCapable'] and state['artworkSide'] > 0, state
                assert len(visible_controls) >= 4, state
                if name == 'reference-music':
                    assert state['surfaceRect'][2:] == [430, 164], state
                    assert state['albumRect'][2:] == [72, 72], state
            elif name == 'no-media':
                assert not state['mediaSeekCapable'] and state['artworkSide'] == 0, state
                assert len(visible_controls) == 0, state
            else:
                assert state['mediaSeekCapable'] and state['artworkSide'] == 0, state
                assert len(visible_controls) >= 4, state

            image_path = folder / f'{name}-{dpi}.png'
            save_test_screenshot(image_path, bounds)
            from PIL import Image
            with Image.open(image_path) as image:
                image.load()
                assert image.width == bounds.right - bounds.left
                assert image.height == bounds.bottom - bounds.top
                assert image.getbbox(), image_path
            record = {'dpi': dpi, 'state': name, 'snapshot': state,
                      'screenshot': str(image_path), 'nonBlank': True}
            dpi_cases.append(record)
            cases.append(record)
            print(f'PASS: DISPLAY2 {dpi} DPI {name}; MiSans and geometry verified')
        finally:
            if hwnd and proc.poll() is None:
                close(proc, hwnd)
            elif proc.poll() is None:
                raise AssertionError(f'owned content fixture {proc.pid} has no closable HWND')
        assert proc.returncode == 0, proc.returncode
    toolbar_cases = [case for case in dpi_cases if case['state'] != 'reference-music']
    baseline = toolbar_cases[0]['snapshot']['surfaceRect']
    assert all(case['snapshot']['surfaceRect'] == baseline for case in toolbar_cases), dpi_cases

(folder / 'results.json').write_text(json.dumps({
    'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
    'monitor': r'\\.\DISPLAY2',
    'cases': cases,
}, indent=2), encoding='utf-8')
