# Troubleshooting

Find the message Quota showed, or the symptom, below. Most messages that mention the log
also give its full path;
[Where Quota keeps its data](user-guide.md#where-quota-keeps-its-data) lists it for each
system.

- [Adding an account](#adding-an-account)
- [Browser sign-in](#browser-sign-in)
- [An account card](#an-account-card)
- [The app](#the-app)
- [Getting help](#getting-help)

## Adding an account

**"The provider did not accept this sign-in, so nothing was added."** The sign-in Quota
read or the key you pasted was refused. Sign in again with the provider's own tool, or
create a new key, then press **Connect** again:

| Provider        | Sign in again with                                              |
| --------------- | --------------------------------------------------------------- |
| Codex           | `codex login`                                                   |
| Claude          | `claude`, then sign in to Claude Code                           |
| Cursor          | The Cursor app                                                  |
| OpenCode Go     | A new key from opencode.ai/auth, or `opencode auth login`       |
| OpenRouter      | A new key from openrouter.ai/settings/keys                      |
| Z.ai, MiniMax   | A new key from the provider's console                           |
| Kimi for Coding | A new key from kimi.com/code/console, or `kimi`, then `/login`  |
| Ollama Cloud    | A new key from ollama.com/settings/keys, or `ollama signin`     |
| Grok, Muse Code | Start the browser sign-in again, or `grok login` / `muse login` |

**"The provider declined this account, so nothing was added."** The sign-in works, but the
account has no subscription Quota can read, for example an inactive or expired plan.

**The Verify step shows the wrong account.** Quota reads whichever account the provider's
own tool is signed in to. Choose **Not this account**, sign that tool in to the account
you want, and start again. To monitor both, add each with its own sign-in or key.

**"The provider's answer was incomplete or inconsistent, so nothing was added."** The
provider answered without something Quota needs, such as the account's identity. Try
again; if it keeps happening, [report it](#getting-help) with the provider's name and the
lines around the failure in the log.

**"The provider answered in a format this version of Quota cannot read yet."** The
provider changed its answer. Check for a newer Quota version, and
[report it](#getting-help) if you have the latest.

**"The provider reported a temporary failure"** or **"The provider is asking for less
traffic"** Wait a minute and try again. Some providers, Muse Code in particular, limit how
often their usage can be read.

**"The operating system key store is unavailable"** or **"refused to keep the
credential"** Quota keeps API keys and browser sign-ins in the system credential store. On
Linux, make sure a keyring service (GNOME Keyring or KWallet) is installed and unlocked,
then try again. On Windows and macOS, signing out and back in usually clears it.

**The provider does not finish in time** ("did not finish discovering the account" or
"verifying the quota in time") Check your internet connection and try again. Nothing was
added.

## Browser sign-in

These apply to Grok and Muse Code.

**The browser did not open.** After ten seconds Quota shows the address and the code. Open
the address yourself, enter the code if asked, and approve. Quota keeps waiting.

**I approved, but the browser page just says I can close it.** That is expected. Nothing
comes back to the browser; Quota notices the approval by itself within a few seconds and
moves to **Verify**. Switch back to the Quota settings window.

**"The sign-in code expired before it was entered."** Codes last a few minutes. Press
**Connect** again for a new one.

**"The sign-in was declined on the provider's page."** The sign-in was refused or
cancelled on the provider's page. Start again if that was a mistake.

**"The provider did not answer the sign-in in time."** Try again; if the browser page
never loads, check that the provider's site is reachable.

## An account card

**The card says Reconnect.** The sign-in expired or was refused. Sign in again with the
provider's own tool (see the table above), then choose **Reconnect** on the card or in
**Settings → Accounts**. For an API key that was revoked, **Disconnect** the account and
add it again with a new key.

**The readings do not change when I press refresh.** Each provider has a minimum time
between checks, and Quota waits longer when a provider asks it to slow down. The message
after refresh says when the account can next be checked. Also check that **Settings →
General → Pause monitoring** is off and the account's switch in **Settings → Accounts** is
on.

**Rate limited, Offline or Check failed** Quota retries by itself, waiting longer after
each failure. If an account stays in **Check failed**, look in the log for the provider's
name and [report it](#getting-help).

**Partially reported** The provider left out a window it normally reports. It usually
clears at the next reading.

**A window I expect is missing.** Quota shows only the windows the provider reports. Muse
Code, for example, reports no usage while its 5-hour window is idle, so the card can be
empty until you use it.

**Stale** The last reading is old: monitoring is paused, the account is waiting on a
provider's minimum interval or backoff, or the computer was offline or asleep.

## The app

**I closed Quota and it is still running.** Closing a window hides it to the tray. Use
**Exit** in the tray menu to stop it.

**I cannot find the window.** Click the tray icon. If the mini widget is on, it replaces
the full window; use its expand button, **Open the full window**, or clear **Mini widget**
in the tray menu.

**There is no tray icon on Linux.** Quota uses an AppIndicator tray icon. Some desktops,
such as GNOME, need an extension for it (for example "AppIndicator and KStatusNotifierItem
Support"). Starting Quota again brings its window forward even without the icon.

**Windows or macOS refuses to open Quota after I download it.** The packages are not
signed with an operating-system certificate yet; see [Install](user-guide.md#install).

**An update failed.** Quota keeps running the version you have and offers the update again
after a restart. You can also download the new version from the
[releases page](https://github.com/ShayGus/quota/releases/latest) and install it over the
old one.

## Getting help

The quickest way is **Report a bug**: in the tray menu, behind the bug button in the
header, beside the mini widget's **Open the full window**, or in **Settings →
Diagnostics**. It offers two choices:

- **Open an issue on GitHub** opens the bug form with your Quota version, operating
  system, view and connected providers already filled in.
- **Copy a prompt for an AI agent** copies a prompt for an AI coding agent, which asks you
  about the problem, shows you the issue for approval and files it for you.

Neither includes account names, keys or file paths. To add more evidence:

1. Turn on **Settings → Privacy → Hide account names** before you take screenshots.
2. Use **Settings → Diagnostics → Export diagnostics** to save a summary of Quota's state.
   It contains no account names, credentials or provider answers, but look through it
   before you share it.
3. Open the log ([where it is](user-guide.md#where-quota-keeps-its-data)) and copy the
   lines around the problem. Check them for anything personal first.
4. If you open an issue by hand instead, use the
   [bug form](https://github.com/ShayGus/quota/issues/new?template=bug_report.yml) and
   include what you did, what you expected, what happened, your Quota version (shown at
   the bottom of Settings), and your operating system.
