"""Smoke-check the packaged native island and floating player with isolated data."""
import ctypes as c
from ctypes import wintypes as w
import pathlib
import subprocess
import sys
import time
import psutil
from PIL import ImageGrab

root = pathlib.Path(__file__).resolve().parents[2]
exe = pathlib.Path(sys.argv[1]).resolve()
out = root / 'dist' / 'verification'
out.mkdir(parents=True, exist_ok=True)
u = c.windll.user32
u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.IsWindowVisible.argtypes = [w.HWND]
callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
proc = subprocess.Popen([
    str(exe), '--test-fixture', '--test-cover', '--open-floating', '--benchmark',
    '--reduced-motion', '--test-monitor', r'\\.\DISPLAY2',
    '--settings-path', str(out / 'native-isolated-settings.json'), '--exit-after', '6',
])
try:
    found = {}
    for _ in range(80):
        @callback_type
        def collect(hwnd, _):
            owner = w.DWORD()
            u.GetWindowThreadProcessId(hwnd, c.byref(owner))
            if owner.value == proc.pid and u.IsWindowVisible(hwnd):
                name = c.create_unicode_buffer(128)
                u.GetClassNameW(hwnd, name, len(name))
                found[name.value] = hwnd
            return True
        u.EnumWindows(collect, 0)
        if {'IsleNativePrototype', 'IsleNativeFloatingPlayer'} <= found.keys():
            break
        assert proc.poll() is None, 'Native app exited before showing both windows'
        time.sleep(.05)
    assert {'IsleNativePrototype', 'IsleNativeFloatingPlayer'} <= found.keys(), found
    assert not psutil.Process(proc.pid).children(recursive=True), 'Unexpected child runtime'
    time.sleep(.3)
    rect = w.RECT()
    u.GetWindowRect(found['IsleNativeFloatingPlayer'], c.byref(rect))
    assert rect.right - rect.left >= 200 and rect.bottom - rect.top >= 200
    image = ImageGrab.grab((rect.left, rect.top, rect.right, rect.bottom), all_screens=True)
    image.save(out / 'native-floating-package.png')
    pixel = image.convert('RGB').getpixel((image.width - 20, image.height - 20))
    assert max(pixel) < 80, f'Floating player background was not painted: {pixel}'
    assert proc.wait(timeout=10) == 0
    print('Native package passed: island + floating HWND, painted background, no child runtime')
finally:
    if proc.poll() is None:
        proc.terminate()
        proc.wait(timeout=5)
