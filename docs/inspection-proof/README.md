# Inspection proof

These files were captured from the running app on 2026-10-02, on WSL2 with WSLg, through
the development-only inspection plugin. They are the evidence behind the table in
[docs/inspecting-the-app.md](../inspecting-the-app.md#what-was-verified-on-this-machine).

| File                      | Tool that produced it                                   |
| ------------------------- | ------------------------------------------------------- |
| `overview-screenshot.jpg` | `take_screenshot`, window `overview`, `max_width: 1400` |
| `console-log.txt`         | `query_logs`                                            |
| `overview-dom.html`       | `query_page` with `mode: "html"`, window `overview`     |
| `element-map.txt`         | `query_page` with `mode: "map"`, window `overview`      |

The screenshot is 810x720, the real size of the overview window. It needed two things this
display does not give for free — `GDK_BACKEND=x11` before `bun run inspect`, and
`docs/publish-x11-client-list.py 0x600010` after the overview appeared. Both are explained
in [Screenshots on WSLg](../inspecting-the-app.md#screenshots-on-wslg). The image agrees
with `element-map.txt` line for line: `Quota`, `0 accounts · 0 providers`, `Floating`,
`Least remaining first`, `No account matches the current filter or search text.`,
`0 / 0 shown`, `Monitoring active`.

A launch opens only overview. Settings is created hidden and opens when a person asks for it.

`console-log.txt` is the important one. `push_log` is the single command that has to
travel through the Tauri capability allowlist before the plugin can see it; the plugin
counts a denied push as an error. Seeing real `console.*` lines here means the allowlist
grant works end to end, which is what the `mcp:default` capability in
`src-tauri/capabilities/agent-inspection-capability.json` exists for.

## Reproducing

```bash
export GDK_BACKEND=x11
bun run inspect
```

then, from another shell, with the app running:

```bash
test -S "${TMPDIR:-/tmp}/tauri-mcp.sock" && echo "socket up"
xwininfo -root -tree | grep '"Quota"'
docs/publish-x11-client-list.py 0x600010   # the id from the line above
bun x tauri-mcp-server
```

`initialize`, then `notifications/initialized`, then `tools/call` with `take_screenshot`,
`query_page`, `query_logs` and `read_text`. Every tool needs `window_label: "overview"` or
`"settings"`; the plugin's default is `main`, which this application does not have.
