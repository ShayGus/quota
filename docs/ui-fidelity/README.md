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

- **Features of the earlier specification.** Specification v0.5 and the 810 px v0.3
  wireframe call for account search, a visible/total account count, the side-by-side
  comparison table, Fit all accounts, Reset window position, and a row density setting.
  The approved wireframe has none of them beyond its filter counts and the rings/bars
  layout switch. The owner confirmed on 2026-10-02 that they stay removed, so the window
  commands and the density preference that served them are removed as well; a saved
  preferences file that still carries a density loads normally.
- **Floating and always on top.** The wireframe's "Pin as a floating window" and "Keep
  pinned window on top" read as one feature. By the owner's direction the header button
  floats or docks the window ("Float as a separate window", "Dock to the tray", with a
  "Floating" label), and the setting is "Always on top", which applies in both modes. The
  two remain independent: changing one never changes the other.
- **Window placement.** The overview keeps its position and size between restarts, by the
  owner's decision, so a saved width is not reset to 440 px. Settings is owned by the
  overview and opens centred over it, on its screen; with the overview hidden it opens
  beside the tray.
- **Order.** The wireframe lists accounts in the order they were added and offers up and
  down buttons. Quota ranks accounts by their least remaining allowance (spec 4.1), and
  the host has no reorder command, so the order buttons are shown disabled with that
  reason.
- **Connection.** By the owner's direction, adding an account happens in the settings
  window, on its add-account page, rather than in the popover the wireframe draws: every
  Add account opens it there. The wizard uses `begin_connection`, `confirm_connection`,
  and `cancel_connection`; a verified attempt is held by the host as a pending candidate,
  so nothing is saved and no monitoring starts until it is added. Verify replaces the
  wireframe's confirmation checkbox, also by the owner's direction, with one decision: a
  card with the verified account, its workspace and plan, and each reading, the nickname,
  and **Add <provider> account** or **Not this account**, which discards it and says how
  to switch the provider's own sign-in. Cancel, closing the settings window, or restarting
  the app discards the candidate. A provider with no tool of its own on the computer, such
  as OpenRouter, asks for an API key on Connect instead, and the badge reads API KEY. Grok
  and Muse Code sign in on the provider's own page in the browser: Connect shows the code
  to enter there, and the badge reads BROWSER SIGN-IN.
- **Pay-as-you-go cards.** An account with no included allowance, such as OpenRouter's,
  draws its credit balance and spend limits as its rings, since the wireframe has no such
  account and an empty card would hide what the account has.
- **Provider marks.** The wireframe draws Codex as the text `>_`. By the owner's direction
  the tile shows Codex's own mark, the rounded blossom around a `>_` prompt that the Codex
  app uses, in the tile's text colour. Providers the wireframe does not have show their
  own marks the same way: Cursor, Ollama, OpenRouter, Z.ai, MiniMax, Kimi, Grok, and
  Meta's for Muse Code. Claude and OpenCode Go keep their glyphs.
- **Launch at login** registers a login item through the autostart plugin. A launch at
  sign-in starts quietly in the tray, and the switch shows what the system reports.
- **Local history retention** offers the wireframe's Disabled, 7 days, and 30 days, plus
  Keep indefinitely. This build has no retention sweep, so 7 and 30 days are shown but
  refused (the owner's R18 decision).
- **Clear accounts** becomes Clear history: Quota clears stored quota history for every
  account, and disconnecting stays an explicit per-account action.
- **Diagnostics** lists each account's sanitized connection and fetch state in place of
  the prototype's interaction log. Export writes `quota-diagnostics-settings.json` to the
  host's diagnostics folder and states where; the file never holds account identities, so
  there is no switch to include them.
- **Tray.** By the owner's direction the tray menu has three items, Settings, Show App,
  and Exit, with the wireframe's icons; a left click always opens the app, and closing a
  window hides it to the tray. Refresh, pin, and pause stay in the popover and settings.
  The tray icon is the Quota mark in the wireframe's light or dark accent, with the
  attention dot and tooltip ("Quota · Claude · 5h low") driven by the snapshot. Quota runs
  as a single instance.
- **Notification preview** stays inside the settings window, because no host preview
  command exists, and is built from the account closest to exhaustion, with its real
  reading, rather than the wireframe's sample.
- **Refresh feedback.** As in the wireframe, a refresh answers with a toast. A deferred
  read is stated there ("Manual refreshes for … are deferred. Next eligible read in 4m.")
  rather than as a banner, and a command the host refuses is stated the same way.

## Live verification

The real application was checked with `bun run inspect`, which exposes the overview
webview to an agent. [`tools/ui-parity`](../../tools/ui-parity/README.md) runs the same
measurement script in the live webview and in the wireframe (headless Chrome at the same
125% scale). For each listed selector it measures the first matching element, reading 15
computed properties (font, colour, spacing, border, radius, shadow and similar) and its
width and height, across ten cases: the overview, quota detail, the wizard's first two
steps, and each settings panel. The recorded results in
[`tools/ui-parity/results`](../../tools/ui-parity/results/) name the commit they were
measured at and show no difference in those measurements. What each case ignores is listed
in its result: data (which accounts exist and their values), the width the native border
takes, and, for the settings panels, width, height and margins, because the settings route
is measured inside the overview window. Repeated elements after the first, and settings
sizes, are therefore not compared; the paired screenshots cover those. Leaving the wizard
was driven live as well: after a real verification reached Verify, Escape sent
`cancel_connection` and never `confirm_connection`, and the account list was unchanged.
Native behaviour was driven with real input: a floating window drags by its header,
docking anchors the popover above the taskbar, Escape steps back and then hides, settings
Details and Add account open the popover, and the tray's icon, tooltip, and menu were read
from Windows. Each tray menu item was chosen: Settings opens settings, Show App restores
the popover, and Exit ends the process. A left click on the icon opens the app and a
second click keeps it open. The popover's ×, Alt+F4, and the settings window's × hide to
the tray while the process keeps running. A second launch keeps one process and brings the
app forward, and a `--autostart` launch stays hidden. Launch at login wrote the `Quota`
entry in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` and removed it again. The
popover's height follows its content between 320 and 760 pixels, as the wireframe's does:
the renderer reports its content height and the host resizes the window inside the work
area, keeping a tray popover against the screen edge the tray is on.
