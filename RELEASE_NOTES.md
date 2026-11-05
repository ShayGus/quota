# Release notes

What changed in each version, in words for the people using Quota. The newest goes first.

Quota builds this file in, and shows a version's section once, the first time that version
runs after an update. The GitHub release uses the same words. Each section is a
`## version` heading and a list, in plain text. A release can't be built without a section
for its version.

## 0.1.11

- The mini widget's compact layout is now a small list: one row per account, in columns
  that line up.
- The widget is see-through until you point at it, so it hides less of what's behind it.
- A coloured rim makes the widget easy to spot. It turns amber and flashes when a limit
  runs low.
- Lost the widget? Click the tray icon, choose Find widget, or press Ctrl+Alt+Q. You can
  change the shortcut in Settings.
- The widget snaps to the edges of your screen.
- Click a row for its details, and open the full window from there.
- The widget leaves out limits a provider doesn't report, instead of showing a dash.
- Codex credits read 62,500 instead of 62,500.000000000.
- Report a bug moved from the widget to the tray menu. It's still in the full window and
  in Settings too.
- After an update, Quota now shows you what changed, as it's doing right now.

## 0.1.10

- TypeSafe reads your balance from the same place as its own billing page.
- Ollama Cloud shows your plan's limits again.
