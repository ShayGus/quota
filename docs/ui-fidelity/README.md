# UI fidelity pairs

These recorded captures compare real Quota screens with the approved wireframe,
`quota-wireframe.html` ("Quota — Interactive HTML wireframe"), in the same state. The
wireframe is maintained outside this repository. It draws Quota as a 440 px tray popover
with one card per account, a 780 px settings window, and an Add account wizard inside the
popover. It replaces the earlier 810 px table wireframe (v0.3) that previous captures
compared against.

| Wireframe                 | App                         | State                             |
| ------------------------- | --------------------------- | --------------------------------- |
| `wireframe-overview.png`  | `app-overview.png`          | All accounts, donut layout, light |
| `wireframe-dark.png`      | `app-dark.png`              | The same, dark                    |
| `wireframe-compact.png`   | `app-compact.png`           | Compact layout                    |
| `wireframe-detail.png`    | `app-detail.png`            | Claude 5-hour quota detail        |
| `wireframe-reconnect.png` | `app-reconnect.png`         | Claude connection expired         |
| `wireframe-wizard.png`    | `app-wizard.png`            | Add account, Connect step         |
| `wireframe-empty.png`     | `app-empty.png`             | First launch                      |
| `wireframe-settings.png`  | `app-settings-general.png`  | Settings, General                 |
| `wireframe-accounts.png`  | `app-settings-accounts.png` | Settings, Accounts                |

The wireframe images are the wireframe page itself, with its desktop preview around the
popover. The app images are the real renderer at the popover's 440 px width, or the
settings window's 780 px width, driven by one deterministic snapshot with the wireframe's
sample accounts. The wireframe's ClinePass example is drawn with OpenCode Go, the provider
this build supports with a monthly window.

## Deliberate differences

The wireframe is a prototype. Where its words describe the prototype ("sample data",
"Simulate connection", "Clear demo accounts"), the app states what the product actually
does, in the same place and at the same length.

- **Order.** The wireframe lists accounts in the order they were added and offers up and
  down buttons. Quota ranks accounts by their least remaining allowance (spec 4.1), and
  the host has no reorder command, so the order buttons are shown disabled with that
  reason.
- **Connection.** The host saves an account as soon as the provider verifies it. The
  Verify step identifies the new account in the snapshot, asks the person to confirm it
  and name it, and removes it again if they go back or cancel, which gives the wireframe's
  confirm-before-adding behaviour.
- **Launch at login** registers a login item through the autostart plugin. A launch at
  sign-in starts quietly in the tray, and the switch shows what the system reports.
- **Local history retention** offers the wireframe's Disabled, 7 days, and 30 days, plus
  Keep indefinitely. This build has no retention sweep, so 7 and 30 days are shown but
  refused (the owner's R18 decision).
- **Clear accounts** becomes Clear history: Quota clears stored quota history for every
  account, and disconnecting stays an explicit per-account action.
- **Diagnostics** lists each account's sanitized connection and fetch state in place of
  the prototype's interaction log, and keeps the Include identities export switch.
- **Notification preview** stays inside the settings window; no host preview command
  exists.
- **Tray.** By the owner's direction the tray menu has three items, Settings, Show App,
  and Exit, with the wireframe's icons; a left click always opens the app, and closing a
  window hides it to the tray. Refresh, pin, and pause stay in the popover and settings.
  The tray icon is the Quota mark in the wireframe's light or dark accent, with the
  attention dot and tooltip ("Quota · Claude · 5h low") driven by the snapshot. Quota runs
  as a single instance.
- **Notification preview** is built from the account closest to exhaustion, with its real
  reading, rather than the wireframe's sample.
- **Refresh feedback.** As in the wireframe, a refresh answers with a toast. A deferred
  read is stated there ("Manual refreshes for … are deferred. Next eligible read in 4m.")
  rather than as a banner, and a command the host refuses is stated the same way.

## Live verification

The real application was checked with `bun run inspect`, which exposes the overview
webview to an agent. The same measurement script ran in the live webview and in the
wireframe (headless Chrome at the same 125% scale) and compared the computed font, colour,
spacing, border, radius, and size of every shared element: popover header, toolbar, cards,
rings, footer, compact rows, expanded limits, quota detail, the wizard's first two steps,
every settings panel, the dialogs, and the notification preview. The remaining differences
are data (which accounts exist and their values) and the text listed above. Native
behaviour was driven with real input: the pinned header drags the window, unpinning
anchors the popover above the taskbar, Escape steps back and then hides, settings Details
and Add account open the popover, and the tray's icon, tooltip, and menu were read from
Windows. Each tray menu item was chosen: Settings opens settings, Show App restores the
popover, and Exit ends the process. A left click on the icon opens the app and a second
click keeps it open. The popover's ×, Alt+F4, and the settings window's × hide to the tray
while the process keeps running. A second launch keeps one process and brings the app
forward, and a `--autostart` launch stays hidden. Launch at login wrote the `Quota` entry
in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` and removed it again. The
popover's height follows its content up to 760 pixels, as the wireframe's does.
