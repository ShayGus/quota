# Quota

Quota shows how much of your AI subscriptions you have left. It sits in the system tray
and puts every 5-hour, weekly, monthly and model-specific allowance of every account in
one small window, with when each resets and how recently it was checked.

<p>
  <img src="docs/images/overview-rings.png" width="320" alt="The Quota overview: one card per account, with a ring for each allowance window">
  &nbsp;
  <img src="docs/images/widget-rings.png" width="320" alt="The mini widget: every account as a small ring">
</p>

- **Every subscription in one place**: Codex, Claude, Cursor, OpenCode Go, OpenRouter,
  Z.ai, MiniMax, Kimi, Ollama Cloud, Grok and Muse Code, with as many accounts of each as
  you have.
- **Closest to running out first**: cards are ordered by remaining allowance, and the
  Attention filter shows only the accounts that need something from you.
- **Alerts before you hit a limit**: optional notifications at 20%, 10% and 0% left, and
  when a window is available again.
- **A mini widget** that stays on top with every account at a glance.
- **Private by design**: no server, no account, no tracking. Quota talks only to the
  providers you add, reads their existing sign-ins without changing them, and keeps keys
  in your system's credential store.

Quota runs on Windows, Linux and macOS.

## Install

Download the package for your system from the
[latest release](https://github.com/ShayGus/quota/releases/latest):

- **Windows**: `Quota_<version>_x64-setup.exe`
- **Linux**: `Quota_<version>_amd64.AppImage`, or the `.deb` or `.rpm`
- **macOS**: `Quota_<version>_aarch64.dmg` (Apple Silicon) or `Quota_<version>_x64.dmg`
  (Intel)

The packages are not signed with an operating-system certificate yet, so Windows and macOS
ask for confirmation the first time; [Install](docs/user-guide.md#install) explains what
to choose. Installed copies offer each new version as it is published.

## Get started

1. Start Quota. It opens above its tray icon.
2. Choose **Add your first account**, pick a provider, and connect it: Quota reads the
   sign-in the provider's own tool already has, takes an API key, or opens a browser
   sign-in.
3. Check that the account Quota found is the right one, give it a nickname, and add it.

Close the window to keep Quota running in the tray; **Exit** in the tray menu stops it.

## Documentation

**Using Quota**

- [User guide](docs/user-guide.md): installing, adding accounts, the overview, the mini
  widget, every setting, updates, and where your data is kept.
- [Troubleshooting](docs/troubleshooting.md): what each error means and what to do, and
  how to report a problem.
- [Providers](docs/providers.md): exactly which sign-in, endpoint and fields Quota uses
  for each provider.

**Working on Quota**

- [Contributing](CONTRIBUTING.md): set up a checkout, run the app and every check, and the
  rules the checks enforce.
- [Architecture](docs/architecture.md): the crate graph, allowed dependency directions,
  and who owns which state.
- [Platforms](docs/platforms.md): where Windows, Linux and macOS differ, and what macOS
  still needs.
- [Releasing](docs/RELEASING.md): packages, signing, and publishing a release.
- [Acceptance](docs/acceptance.md), [Dependencies](docs/dependencies.md),
  [Exceptions](docs/exceptions.md) and [Inspecting the app](docs/inspecting-the-app.md).

## Development

Quota is a [Tauri 2](https://tauri.app) app: a Rust backend owns provider access, polling,
credentials, storage and notifications, and a React 19 frontend draws the windows. There
is no server component.

With [rustup](https://rustup.rs) and [Bun](https://bun.sh) installed:

```bash
git clone https://github.com/ShayGus/quota.git
cd quota
bun install
bun tauri dev
```

A development build runs as **Quota Dev**, with its own data, sign-ins and login item, so
it never touches an installed copy. [Contributing](CONTRIBUTING.md) has the Linux build
packages, every check, and the tests.

## Reporting a problem

[Open an issue](https://github.com/ShayGus/quota/issues/new).
[Getting help](docs/troubleshooting.md#getting-help) lists what to include and how to
leave out anything personal.

## Licence

MIT. See [LICENSE](LICENSE).
