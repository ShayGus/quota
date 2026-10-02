# UI parity check

Compares the running app with the approved wireframe, `quota-wireframe.html`, element by
element. The wireframe is maintained outside this repository.

The same script, [`measure.js`](measure.js), runs in the app's overview webview and in the
wireframe page. For each listed selector it reads the first matching element's computed
font, colour, spacing, border, radius and shadow properties and its width and height;
later matches of the same selector are not compared. [`cases.mjs`](cases.mjs) lists what
to open in each and which properties differ only by data, such as which account is on
screen. [`compare.mjs`](compare.mjs) writes one result per case to [`results/`](results/)
and exits non-zero when a design difference remains. Results record measurements only,
never on-screen text, so they carry no account data.

## Run it

1. Start the app with inspection: `bun run inspect` (see
   [the inspection guide](../../docs/inspecting-the-app.md)). Leave it on the overview, in
   the dark theme.
2. On Windows, point `TAURI_MCP_SERVER` at a copy of the inspection server patched for the
   plugin's pipe name and set `TAURI_MCP_PIPE`, as
   [the guide's Windows section](../../docs/inspecting-the-app.md#on-windows) describes.
3. From the repository root:

   ```bash
   QUOTA_WIREFRAME=/path/to/quota-wireframe.html bun tools/ui-parity/compare.mjs
   ```

   Name cases to run only some of them, for example `overview detail`. `CHROME` overrides
   the browser executable.

Both sides render at a device scale of 1.25 so fractional borders and heights compare like
for like. Lengths within one pixel of each other count as equal; the wireframe's 440-pixel
popover includes its own border, which the native window draws instead.
