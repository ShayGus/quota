/**
 * Asks Windows itself where a window is, and moves it the way the system does.
 *
 * WebDriver can't place a WebView2 window, and what the widget does when it is
 * moved, it does because the system says it moved. This uses no mouse or
 * keyboard, so it runs on CI's machines like the rest of the suite. It finds a
 * window by its exact title, and only Quota's debug builds have these titles.
 */
import { execFileSync } from "node:child_process";

/** A window's place on the desktop and the usable part of its screen, in pixels. */
export interface NativeWindow {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly area: {
    readonly left: number;
    readonly top: number;
    readonly right: number;
    readonly bottom: number;
  };
}

const HELPER = `
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class NativeWindow {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct INFO { public int Size; public RECT Monitor; public RECT Work; public int Flags; }
  public delegate bool Visit(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Visit v, IntPtr p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int t, uint f);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr m, ref INFO i);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  public static IntPtr Find(string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, p) => {
      var s = new StringBuilder(200);
      GetWindowText(h, s, 200);
      if (IsWindowVisible(h) && s.ToString() == title) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@
[void][NativeWindow]::SetProcessDpiAwarenessContext([IntPtr](-4))
`;

function powershell(script: string): string {
  return execFileSync(
    "powershell",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
    { encoding: "utf8" },
  ).trim();
}

/** Where the window with this exact title is, or `null` when it isn't showing. */
export function nativeWindow(title: string): NativeWindow | null {
  const answer = powershell(`${HELPER}
$h = [NativeWindow]::Find(${JSON.stringify(title)})
if ($h -eq [IntPtr]::Zero) { 'null'; exit 0 }
$r = New-Object NativeWindow+RECT
[void][NativeWindow]::GetWindowRect($h, [ref]$r)
$i = New-Object NativeWindow+INFO
$i.Size = [System.Runtime.InteropServices.Marshal]::SizeOf($i)
[void][NativeWindow]::GetMonitorInfo([NativeWindow]::MonitorFromWindow($h, 2), [ref]$i)
@{ x = $r.L; y = $r.T; width = $r.R - $r.L; height = $r.B - $r.T; area = @{ left = $i.Work.L; top = $i.Work.T; right = $i.Work.R; bottom = $i.Work.B } } | ConvertTo-Json -Compress
`);
  return JSON.parse(answer) as NativeWindow | null;
}

/** Moves the window with this exact title, without resizing or raising it. */
export function moveNativeWindow(title: string, x: number, y: number): void {
  // SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE.
  powershell(`${HELPER}
$h = [NativeWindow]::Find(${JSON.stringify(title)})
if ($h -eq [IntPtr]::Zero) { throw 'no such window' }
[void][NativeWindow]::SetWindowPos($h, [IntPtr]::Zero, ${String(Math.round(x))}, ${String(Math.round(y))}, 0, 0, 21)
`);
}
