# Inspecting the running app from an AI agent

A debug build of Quota can be inspected by an AI agent: a screenshot of either window, the
DOM and console log of overview, and the IPC calls an agent issues through the plugin's
own tools. `manage_ipc` records only those agent-issued calls; ordinary frontend invokes
are not intercepted. This is how a worker checks that the real interface matches a mockup
and that behaviour is correct, instead of guessing from the source.

Inspection is unavailable in shipping builds. See
[Why it cannot reach a release](#why-it-cannot-reach-a-release). What works and what does
not on this particular machine is recorded under
[What was verified on this machine](#what-was-verified-on-this-machine) and
[Screenshots on WSLg](#screenshots-on-wslg).

## What an agent gets

The plugin exposes nineteen tools over a Unix socket. Agent-driven webview tools
(`query_page`, `read_text`, `inspect_element`, input tools, `execute_js`, and `manage_ipc`
invokes) work only in `overview`, using its command permissions. The `settings` window
keeps its own capabilities and installs no guest request handlers, so an event sent to
settings cannot execute a command or JavaScript there. Native screenshots can still
capture either visible window. Webview console forwarding is restricted to overview.

Pass `window_label: "overview"` to tools that target a webview. Their default is `main`,
which this application does not have.

The renderer checks its actual window label and asks the host to authorize a no-op
`mcp:push_ipc` call before importing the guest. Ordinary development runs without the
inspection plugin or capability grant install no guest listeners. Socket clients require
the token; guest events themselves do not authenticate their sender, which is why settings
must not register their handlers.

The ones an agent uses most:

| Tool                                                  | What it answers                                                                                          |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `take_screenshot`                                     | what the window actually looks like                                                                      |
| `query_page`                                          | the element map, the raw DOM, the URL and title, or the pixel coordinates of a CSS selector              |
| `read_text`                                           | the visible text of everything matching a selector                                                       |
| `inspect_element`                                     | the bounding box and the computed styles of one element                                                  |
| `click`, `type_text`, `press_key`, `dispatch_pointer` | drive the interface                                                                                      |
| `query_logs`                                          | the Rust log, the webview `console.*` output, and intercepted dialogs                                    |
| `manage_ipc`                                          | only IPC calls an agent issues through the plugin's tools; ordinary frontend invokes are not intercepted |
| `execute_js`                                          | run anything the structured tools do not cover                                                           |

## Start the app

From the repository root:

```bash
bun run inspect
```

That script is exactly this, and nothing else:

```bash
bun tauri dev --features agent-inspection --config '{"app":{"security":{"capabilities":["overview-capability","settings-capability","agent-inspection-capability"]}}}'
```

Both halves matter.

- `--features agent-inspection` compiles the plugin. Without the feature the build
  registers no inspection plugin and opens no inspection socket. Keep the capability
  override paired with the feature, since it names permissions supplied by that plugin.
- `--config` adds `agent-inspection-capability` to the capability allowlist. Tauri ignores
  a capability file the allowlist does not name, so without this guest listeners stay
  disabled and console forwarding is denied. The allowlist in `src-tauri/tauri.conf.json`
  deliberately keeps the shipping two capabilities only, because the plugin's permissions
  do not exist in a build that does not compile it.

The renderer needs the Vite dev server on port 1420. If that port is taken, move both
ends:

```bash
bun tauri dev --features agent-inspection \
  --config '{"app":{"security":{"capabilities":["overview-capability","settings-capability","agent-inspection-capability"]}},"build":{"devUrl":"http://localhost:1433","beforeDevCommand":"bun run dev --port 1433"}}'
```

A launch opens only `overview`. The `settings` window is created hidden and opens when a
person asks for it.

For `take_screenshot` on WSLg, export `GDK_BACKEND=x11` first. See
[Screenshots on WSLg](#screenshots-on-wslg).

## Connect an agent

The plugin listens on a Unix socket and writes a random authentication token beside it. On
Linux that is:

|        |                                                           |
| ------ | --------------------------------------------------------- |
| Socket | `${TMPDIR:-/tmp}/tauri-mcp.sock`                          |
| Token  | `${TMPDIR:-/tmp}/tauri-mcp.sock.token`, written mode 0600 |

The MCP server finds both on its own, so the configuration is two keys.
[`docs/mcp.example.json`](mcp.example.json) is a complete, working file. Copy it to the
path your agent reads:

| Agent       | Path                                   |
| ----------- | -------------------------------------- |
| Claude Code | `.mcp.json` in the repository root     |
| omp         | `.omp/mcp.json` in the repository root |

```json
{
  "mcpServers": {
    "quota-inspection": {
      "command": "bun",
      "args": ["x", "tauri-mcp-server"]
    }
  }
}
```

`tauri-plugin-mcp-server` is already a pinned dev dependency, so `bun x` resolves it from
`node_modules` without a network fetch. To name the socket and token explicitly instead of
relying on discovery, omp expands a value that starts with `!` as a shell command:

```json
{
  "mcpServers": {
    "quota-inspection": {
      "command": "bun",
      "args": ["x", "tauri-mcp-server"],
      "env": {
        "TAURI_MCP_IPC_PATH": "!printf '%s' \"${TMPDIR:-/tmp}/tauri-mcp.sock\"",
        "TAURI_MCP_AUTH_TOKEN": "!cat \"${TMPDIR:-/tmp}/tauri-mcp.sock.token\""
      }
    }
  }
}
```

MCP servers load when the agent starts. Restart the agent after writing the file.

## Check the socket without an agent

These checks establish that socket and token files exist; they do not prove that the
current app accepts authenticated requests or that its guest handlers are ready:

```bash
test -S "${TMPDIR:-/tmp}/tauri-mcp.sock" && echo "socket up"
test -s "${TMPDIR:-/tmp}/tauri-mcp.sock.token" && echo "token present"
```

## What was verified on this machine

Run on 2026-10-02 under WSL2 with WSLg, `bun run inspect`, first with the Vite port moved
to 1433 because a sibling checkout held 1420, then on port 1420 with `GDK_BACKEND=x11` for
the screenshot:

| Check                                                                              | Result                                                                                           |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `bun tauri dev --features agent-inspection` starts and the overview window renders | yes                                                                                              |
| `/tmp/tauri-mcp.sock` exists, mode `0600`                                          | yes                                                                                              |
| `/tmp/tauri-mcp.sock.token` exists, mode `0600`                                    | yes                                                                                              |
| `bun x tauri-mcp-server` reaches the socket with the token                         | yes                                                                                              |
| `tools/list`                                                                       | 19 tools                                                                                         |
| `query_page` `mode: "map"`                                                         | the real element tree with refs                                                                  |
| `query_page` `mode: "html"`                                                        | about 27 KB of real DOM                                                                          |
| `query_logs`                                                                       | real `console.*` output from overview and the pre-created hidden settings window                 |
| `take_screenshot`                                                                  | a JPEG of the overview window (recorded at the former 810x720 size), in `docs/inspection-proof/` |

This capture predates the overview-only inspection scope; current sessions forward webview
console logs only from overview. The console output proves the original capability grant
worked: console forwarding uses the capability-gated `push_log` command, and without
`agent-inspection-capability` the plugin records `ok: 0, err: N` instead.

The captured files are in [`docs/inspection-proof/`](inspection-proof/).

## Screenshots on WSLg

A screenshot needs two things this display does not give for free: the app must run on
X11, and the X server must publish a client list. Each is one command.

```bash
export GDK_BACKEND=x11
bun run inspect
```

Without `GDK_BACKEND=x11` the app runs on Wayland, and its windows never appear on
`DISPLAY=:0`, so a capture has nothing to read. Every other tool in this document works on
either backend; only `take_screenshot` needs this one.

`xcap` 0.9.8, the library behind `take_screenshot`, finds top-level windows only through
the EWMH atom `_NET_CLIENT_LIST_STACKING`. WSLg's Weston helper claims
`_NET_SUPPORTING_WM_CHECK` but never publishes the client list, because WSLg runs no EWMH
window manager, so the plugin reports `Window operation failed: Window not found`. Its
Wayland fallback asks the XDG desktop portal, and WSLg runs no portal. Publishing the atom
stands in for the window manager:

```bash
xwininfo -root -tree | grep '"Quota"'
docs/publish-x11-client-list.py 0x600010
```

`publish-x11-client-list.py` writes only `_NET_CLIENT_LIST_STACKING` on the root window
and exits; the values stay. It needs only `libX11.so.6`, no Python packages. The X server
keeps them after the process exits, so it is a one-off per launch, because the window ids
change every launch.

Pass the window ids to capture; both can be published together. The inspection plugin
disables application-name matching and resolves the requested window label to its distinct
title: `Quota` for `overview`, and `Quota settings` for `settings`. Show the requested
window before capturing it.

To check window selection, show settings and publish both window ids. Request one
`take_screenshot` with `window_label: "overview"` and another with
`window_label: "settings"`; each must show the corresponding interface. Publish the same
ids in reverse order and repeat; neither requested interface should change.

`take_screenshot` scales the result to `max_width`, which defaults to 512. Pass
`max_width` above the window width — 1400 for both windows — to get the image at its real
size.

## Native libraries a Linux debug build needs

The plugin's screenshot stack needs three native development dependencies that are not in
the base WSLg image and that `cargo` cannot find on its own:

| Library                               | Needed by                                        | Symptom when missing                                                                                     |
| ------------------------------------- | ------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| `libpipewire-0.3` headers and library | `libspa-sys`, through `xcap`                     | `failed to run custom build command for libspa-sys v0.10.1` … `Cannot find libraries: … libpipewire-0.3` |
| `libclang`                            | `bindgen`, which generates the PipeWire bindings | `Unable to find libclang`                                                                                |
| `libgbm.so` link-time symlink         | `libwayshot-xcap`                                | `rust-lld: error: unable to find library -lgbm`                                                          |

On Debian and Ubuntu: `sudo apt install libpipewire-0.3-dev libclang-dev libgbm-dev`.

Without `sudo`, unpack the packages into a local prefix and point the build at it. These
are the exact values used on this machine:

```bash
# pipewire headers, library and pkg-config file, in $HOME/.local/pipewire
export PKG_CONFIG_PATH="$HOME/.local/pipewire/lib/x86_64-linux-gnu/pkgconfig"
export LIBRARY_PATH="$HOME/.local/pipewire/lib/x86_64-linux-gnu"

# libclang, in $HOME/.local/llvm/lib
export LIBCLANG_PATH="$HOME/.local/llvm/lib"
export LD_LIBRARY_PATH="$HOME/.local/pipewire/lib/x86_64-linux-gnu:$LIBCLANG_PATH"

# libclang has no builtin include directory of its own, and the generated
# bindings include <stdbool.h>, which lives in the installed GCC's headers.
export BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/gcc/x86_64-linux-gnu/15/include"
```

`libgbm.so.1` is present in the base image but the development symlink is not, so one
link-time symlink into the same directory finishes the job:

```bash
ln -sf /usr/lib/x86_64-linux-gnu/libgbm.so.1 \
  "$HOME/.local/pipewire/lib/x86_64-linux-gnu/libgbm.so"
```

Only builds selecting `agent-inspection` need these extra dependencies. The ordinary
`cargo build --release`, `cargo test`, and clippy commands do not select it.

## Why it cannot reach a release

Three independent stops keep inspection out of the shipping application:

1. `tauri-plugin-mcp` is an `optional` dependency behind the non-default
   `agent-inspection` feature, so Cargo does not compile or link it unless that feature is
   selected. The gate fails if `optional = true` is dropped.
2. Ordinary release feature selections do not activate it. The gate resolves feature
   aliases, unifies incoming selections for each local package, and follows optional
   dependency activation and forwarding. It rejects any resulting activation of
   `agent-inspection` or the plugin, and also rejects a missing feature declaration.
3. `src-tauri/src/bootstrap.rs` registers the plugin behind
   `#[cfg(all(debug_assertions, feature = "agent-inspection"))]`, so even
   `--features agent-inspection` cannot register it in a release build. The plugin also
   refuses to open its socket in a release build on its own.

Explicitly selecting `--features agent-inspection` in a release build can still compile
the dependency and its native libraries; the debug guard prevents registration, not
dependency compilation. Shipping builds must keep the feature unselected.

`cargo xtask check-release` runs entirely in Rust. It checks the manifest rules above,
requires the desktop dependency to inherit the root workspace's full-commit git pin, and
rejects inspection source overrides through `[patch]` or `[replace]`, including renamed
entries. It also rejects inspection capability grants outside overview and guest imports
outside an `import.meta.env.DEV` guard. These checks have regressions in
[`xtask/tests/check_release.rs`](../xtask/tests/check_release.rs); the host's debug
registration guard lives in [`src-tauri/src/bootstrap.rs`](../src-tauri/src/bootstrap.rs).

The frontend CI job runs `bun run check:release:renderer`, which runs `bun run build`. The
Vite build check rejects inspection modules or imports remaining in emitted chunks,
including renamed packages. The guest import in `src/main.tsx` is removed by the
development guard. Bundle-check regressions live in
[`tests/release-renderer.test.ts`](../tests/release-renderer.test.ts), and listener
authorization regressions in [`tests/inspection.test.ts`](../tests/inspection.test.ts).

The licence, and why `deny.toml` keeps an empty `allow-git`, are recorded in
[the dependency record](dependencies.md).
