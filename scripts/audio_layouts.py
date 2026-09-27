"""Read-only device-list layout and blank-collapse checks at each island edge."""
import ctypes as c
from ctypes import wintypes as w
import json
import subprocess
import time
from PIL import ImageGrab
from interaction import ROOT, OUT, wait_window, close, u

results = []
for layout in ['floating', 'top', 'right', 'bottom', 'left']:
    report = OUT / ('audio-layout-' + layout + '.json')
    args = [str(ROOT/'target/release/isle-native.exe'), '--live-audio', '--page', 'volume',
            '--paused', '--reduced-motion', '--benchmark', '--log', str(report)]
    if layout != 'floating':
        args += ['--attached', '--edge', layout]
    proc = subprocess.Popen(args)
    hwnd = wait_window(proc)
    def snap():
        before = report.stat().st_mtime_ns if report.exists() else 0
        u.PostMessageW(hwnd, 0x803c, 0, 0)
        for _ in range(100):
            if report.exists() and report.stat().st_mtime_ns != before:
                try:
                    return json.loads(report.read_text())
                except json.JSONDecodeError:
                    pass
            time.sleep(.02)
        raise AssertionError('diagnostic timeout')
    try:
        # Wait for the real device snapshot before navigating enabled controls.
        for _ in range(40):
            state = snap()
            if state['audioEndpointAlive'] and state['audioDevices'] > 0:
                break
            time.sleep(.1)
        else:
            raise AssertionError('audio endpoint unavailable')
        for _ in range(10):
            u.PostMessageW(hwnd, 0x100, 9, 0)
        u.PostMessageW(hwnd, 0x100, 13, 0)
        time.sleep(.2)
        state = snap()
        assert state['audioDeviceMenu'] and state['audioWrites'] == 0, (layout, state)
        bounds = w.RECT()
        u.GetWindowRect(hwnd, c.byref(bounds))
        ImageGrab.grab((bounds.left, bounds.top, bounds.right, bounds.bottom)).save(
            OUT/('audio-layout-' + layout + '.png'))
        # Lower padding inside the settled outline is empty in the device menu.
        scale = state['scale']
        # Region bounds include the 32-DIP shoulder extent at side attachments.
        # Keyboard Escape is separately covered; click below the last device row.
        g = c.windll.gdi32
        g.CreateRectRgn.restype = w.HRGN
        u.GetWindowRgn.argtypes = [w.HWND, w.HRGN]
        g.GetRgnBox.argtypes = [w.HRGN, c.POINTER(w.RECT)]
        g.DeleteObject.argtypes = [w.HANDLE]
        region = g.CreateRectRgn(0, 0, 0, 0)
        try:
            assert u.GetWindowRgn(hwnd, region) > 1
            shape = w.RECT()
            g.GetRgnBox(region, c.byref(shape))
        finally:
            g.DeleteObject(region)
        x = (shape.left + shape.right)//2
        y = shape.bottom - round((64 if layout in ['left', 'right'] else 36)*scale)
        position = (y << 16) | x
        u.PostMessageW(hwnd, 0x201, 1, position)
        u.PostMessageW(hwnd, 0x202, 0, position)
        time.sleep(.2)
        close(proc, hwnd)
        final = json.loads(report.read_text())
        assert final['livePages'] == 0 and final['audioWrites'] == 0, (layout, final)
        results.append({'layout': layout, 'deviceCount': state['audioDevices'],
                        'menuMounted': True, 'blankCollapsed': True, 'audioWrites': 0})
    finally:
        close(proc, hwnd)
(OUT/'audio-layout-results.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
print('PASS: read-only device menu and blank collapse in all 5 layouts')
