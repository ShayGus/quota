# Inspection proof

These files were captured before inspection was restricted to overview, from the running
app on 2026-10-02, on WSL2 with WSLg, through
the development-only inspection plugin. They are the evidence behind the table in
[docs/inspecting-the-app.md](../inspecting-the-app.md#what-was-verified-on-this-machine).

| File                      | Tool that produced it                                   |
| ------------------------- | ------------------------------------------------------- |
| `overview-screenshot.jpg` | `take_screenshot`, window `overview`, `max_width: 1400` |
| `console-log.txt`         | `query_logs`                                            |
| `overview-dom.html`       | `query_page` with `mode: "html"`, window `overview`     |
| `element-map.txt`         | `query_page` with `mode: "map"`, window `overview`      |

The screenshot is 810x720, the real size of the overview window. The required display setup
is owned by [Screenshots on WSLg](../inspecting-the-app.md#screenshots-on-wslg). The image agrees
with `element-map.txt` line for line: `Quota`, `0 accounts · 0 providers`, `Floating`,
`Least remaining first`, `No account matches the current filter or search text.`,
`0 / 0 shown`, `Monitoring active`.

`console-log.txt` records console forwarding through the capability-gated `push_log`
command. The captured `console.*` lines show that the original
allowlist grant worked at capture time. They include settings output from before the
scope restriction and do not validate the current listener authorization boundary.

## Reproducing

Follow [Start the app](../inspecting-the-app.md#start-the-app),
[Connect an agent](../inspecting-the-app.md#connect-an-agent), and
[Screenshots on WSLg](../inspecting-the-app.md#screenshots-on-wslg).

`initialize`, then `notifications/initialized`, then `tools/call` with `take_screenshot`,
`query_page`, `query_logs` and `read_text`. Window labels and current scope are owned by
[What an agent gets](../inspecting-the-app.md#what-an-agent-gets).
