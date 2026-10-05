# Quota

Quota shows how much of your AI subscriptions you have left. It sits in the system tray
and puts every 5-hour, weekly, monthly and model-specific allowance of every account in
one small window, with when each resets and how recently it was checked.

<table>
  <tr>
    <td valign="top" width="50%">
      <img src="docs/images/overview-rings.png" alt="The Quota window: Codex low on its 5-hour window, Cursor rate limited, Claude current"><br>
      <sub><b>The full window</b>, opened from the tray: one card per account, a ring for each allowance window.</sub>
    </td>
    <td valign="top" width="50%">
      <img src="docs/images/widget-rings.png" alt="The mini widget as a strip of rings"><br>
      <sub><b>Mini widget, rings.</b> Every account in a small window that stays on top, wherever you put it.</sub>
      <br><br>
      <img src="docs/images/widget-cards.png" alt="The mini widget as mini cards with bars"><br>
      <sub><b>Mini widget, bars.</b> The same accounts as mini cards.</sub>
      <br><br>
      <img src="docs/images/overview-bars-light.png" alt="The full window in the compact layout, light theme"><br>
      <sub><b>Compact layout</b>, light theme.</sub>
    </td>
  </tr>
</table>

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

## Screenshots

The screenshots use fictional accounts.

<table>
  <tr>
    <td width="33%" valign="top"><img src="docs/images/overview-all.png" alt="Seven accounts, each in a different state"><br><b>Every state at a glance.</b> Low, rate limited, offline, failed, signed out or paused, each with its own badge.</td>
    <td width="33%" valign="top"><img src="docs/images/account-detail.png" alt="Quota detail for a Codex 5-hour window"><br><b>Quota detail.</b> When each window resets, how much is used and left, and where the reading came from.</td>
    <td width="33%" valign="top"><img src="docs/images/attention-filter.png" alt="The Attention filter"><br><b>Attention.</b> Only the accounts that need something from you: low, exhausted, failing or signed out.</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/images/wizard-provider.png" alt="Add a subscription: the list of eleven providers"><br><b>Add an account.</b> Eleven providers, each with the windows it reports.</td>
    <td valign="top"><img src="docs/images/wizard-browser-code.png" alt="Browser sign-in for Grok with a sign-in code"><br><b>Browser sign-in.</b> Approve in your browser; Quota never asks for a password.</td>
    <td valign="top"><img src="docs/images/wizard-verify.png" alt="Add this account? The verified account and its readings"><br><b>Verify first.</b> See the account and its readings before anything is saved.</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/images/settings-notifications.png" alt="Notification settings"><br><b>Notifications</b> at 20%, 10% and 0% left, and when a window is back.</td>
    <td valign="top"><img src="docs/images/settings-privacy.png" alt="Privacy settings"><br><b>Privacy.</b> No server, no tracking, and a switch to hide account names on screen.</td>
    <td valign="top"><img src="docs/images/settings-appearance.png" alt="Appearance settings"><br><b>Appearance.</b> Light, dark or system theme; rings or a compact list.</td>
  </tr>
</table>

The [user guide](docs/user-guide.md) walks through every screen.

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
