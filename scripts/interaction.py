"""Native HWND integration checks. No browser, no changes to installed Isle.

Runs against its own process IDs. Creates a separate neutral probe process for
cross-process hit routing. Artifacts are local and ignored by Git.
"""
import ctypes as c
from ctypes import wintypes as w
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'artifacts'
OUT.mkdir(exist_ok=True)
u, g, k = c.windll.user32, c.windll.gdi32, c.windll.kernel32
u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
PROC = c.WINFUNCTYPE(c.c_ssize_t, w.HWND, w.UINT, w.WPARAM, w.LPARAM)
ENUM = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u.DefWindowProcW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.DefWindowProcW.restype = c.c_ssize_t
u.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD,
                            c.c_int, c.c_int, c.c_int, c.c_int,
                            w.HWND, w.HMENU, w.HINSTANCE, w.LPVOID]
u.CreateWindowExW.restype = w.HWND
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.WindowFromPoint.argtypes = [w.POINT]
u.WindowFromPoint.restype = w.HWND
u.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
u.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SendMessageW.restype = c.c_ssize_t
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
g.CreateSolidBrush.argtypes = [w.DWORD]
g.CreateSolidBrush.restype = w.HBRUSH


class WC(c.Structure):
    _fields_ = [('style', w.UINT), ('proc', PROC), ('extra', c.c_int),
                ('windowExtra', c.c_int), ('instance', w.HINSTANCE),
                ('icon', w.HICON), ('cursor', w.HANDLE), ('background', w.HBRUSH),
                ('menu', w.LPCWSTR), ('name', w.LPCWSTR)]


def find(pid):
    found = []
    @ENUM
    def callback(hwnd, _):
        owner = w.DWORD()
        u.GetWindowThreadProcessId(hwnd, c.byref(owner))
        if owner.value == pid:
            found.append(hwnd)
        return True
    u.EnumWindows(callback, 0)
    return found[0] if found else None


def wait_window(proc):
    for _ in range(100):
        hwnd = find(proc.pid)
        if hwnd:
            time.sleep(.15)
            return hwnd
        if proc.poll() is not None:
            raise AssertionError('process exited before creating HWND')
        time.sleep(.05)
    raise AssertionError('HWND timeout')


def close(proc, hwnd):
    if proc.poll() is None:
        u.PostMessageW(hwnd, 0x10, 0, 0)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.terminate()
            raise


def probe():
    clicks = 0
    @PROC
    def wnd(hwnd, msg, wp, lp):
        nonlocal clicks
        if msg == 0x201:
            clicks += 1
            return 0
        if msg == 0x8001:
            return clicks
        if msg == 2:
            u.PostQuitMessage(0)
            return 0
        return u.DefWindowProcW(hwnd, msg, wp, lp)
    wc = WC()
    wc.proc = wnd
    wc.background = g.CreateSolidBrush(0x383838)
    wc.name = 'IsleCrossProcessProbe'
    assert u.RegisterClassW(c.byref(wc))
    hwnd = u.CreateWindowExW(0x80, wc.name, 'Native test surface', 0x90000000,
                             0, 0, 480, 480, None, None, None, None)
    assert hwnd
    msg = w.MSG()
    while u.GetMessageW(c.byref(msg), None, 0, 0) > 0:
        u.TranslateMessage(c.byref(msg))
        u.DispatchMessageW(c.byref(msg))
    return


def run():
    from PIL import ImageGrab
    results = []
    background = subprocess.Popen([sys.executable, __file__, '--probe'])
    behind = wait_window(background)
    try:
        for attached in [False, True]:
            for edge in ['top', 'right', 'bottom', 'left']:
                name = ('attached-' if attached else 'floating-') + edge
                report = OUT / (name + '.json')
                args = [str(ROOT / 'target/release/isle-native.exe'), '--page', 'music',
                        '--edge', edge, '--reduced-motion', '--paused', '--log', str(report)]
                if attached:
                    args.append('--attached')
                proc = subprocess.Popen(args)
                hwnd = wait_window(proc)
                try:
                    # Device creation is asynchronous with our HWND discovery.
                    # Wait for the renderer to publish its first non-empty region.
                    g.CreateRectRgn.restype = w.HRGN
                    u.GetWindowRgn.argtypes = [w.HWND, w.HRGN]
                    g.GetRgnBox.argtypes = [w.HRGN, c.POINTER(w.RECT)]
                    g.DeleteObject.argtypes = [w.HANDLE]
                    region = g.CreateRectRgn(0, 0, 0, 0)
                    try:
                        for _ in range(100):
                            if u.GetWindowRgn(hwnd, region) > 1:
                                break
                            time.sleep(.05)
                        else:
                            raise AssertionError('native region never became ready')
                        visible = w.RECT()
                        g.GetRgnBox(region, c.byref(visible))
                    finally:
                        g.DeleteObject(region)
                    bounds = w.RECT()
                    u.GetWindowRect(hwnd, c.byref(bounds))
                    u.SetWindowPos(behind, w.HWND(-1), bounds.left, bounds.top,
                                   bounds.right-bounds.left, bounds.bottom-bounds.top, 0x10)
                    u.SetWindowPos(hwnd, w.HWND(-1), 0, 0, 0, 0, 0x13)
                    time.sleep(.1)
                    # Window manager must route this transparent point to another PID.
                    transparent = w.POINT(bounds.left+2, bounds.top+2)
                    target = u.WindowFromPoint(transparent)
                    assert target == behind, (name, 'transparent corner intercepted')
                    center = w.POINT(bounds.left+(visible.left+visible.right)//2,
                                     bounds.top+(visible.top+visible.bottom)//2)
                    assert u.WindowFromPoint(center) == hwnd, (name, 'opaque center missing')
                    before = u.SendMessageW(behind, 0x8001, 0, 0)
                    u.SendMessageW(target, 0x201, 1, (2 << 16) | 2)
                    assert u.SendMessageW(behind, 0x8001, 0, 0) == before+1
                    # Cancelled press must not collapse the music page.
                    u.SendMessageW(hwnd, 0x201, 1, (100 << 16) | 240)
                    u.SendMessageW(hwnd, 0x1f, 0, 0)  # WM_CANCELMODE
                    u.SendMessageW(hwnd, 0x202, 0, (100 << 16) | 240)
                    time.sleep(.05)
                    ImageGrab.grab((bounds.left,bounds.top,bounds.right,bounds.bottom)).save(OUT/(name+'.png'))
                    close(proc, hwnd)
                    data = json.loads(report.read_text())
                    assert data['livePages'] == 1, (name, 'cancelled gesture collapsed page')
                    results.append({'layout': name, 'crossProcessHitRouting': True,
                                    'cancelPreservesPage': True, 'report': data})
                finally:
                    close(proc, hwnd)
    finally:
        close(background, behind)
    (OUT/'interaction-results.json').write_text(json.dumps(results, indent=2))
    print(f'{len(results)} native layout / cross-process routing checks passed')


if __name__ == '__main__':
    probe() if '--probe' in sys.argv else run()
