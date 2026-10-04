# Releasing

A release is one button on GitHub. You give it a version, it builds the Windows, Linux and
macOS packages and the list that lets installed copies update themselves, and it puts them
in a **draft release**. Nothing is public until you press Publish. This page is the whole
procedure, including the one-time setup of the update key.

The workflow is [`.github/workflows/release.yml`](../.github/workflows/release.yml). It
uses two secrets, both for update signing and both described in
[the update key](#the-update-key): the key and its password, and they reach the build
steps and no other step. Beyond them it uses the automatic `GITHUB_TOKEN`, and only the
last job can write.

## What a release contains

| Platform | Package                                                | Who it is for                                           |
| -------- | ------------------------------------------------------ | ------------------------------------------------------- |
| Windows  | Installer `.exe` (NSIS), installs for the current user | Everyone. No administrator rights needed.               |
| Windows  | `.msi`                                                 | Companies that deploy software centrally.               |
| Linux    | `.AppImage`, runs without installing                   | Everyone.                                               |
| Linux    | `.deb`                                                 | Ubuntu and Debian.                                      |
| Linux    | `.rpm`                                                 | Fedora and openSUSE.                                    |
| macOS    | `.dmg`, one for Apple Silicon and one for Intel        | Everyone on a Mac.                                      |
| macOS    | `.app.tar.gz`, one for Apple Silicon and one for Intel | Updater payloads; initial installation uses the `.dmg`. |
| All      | `.sig` file beside each updater payload                | Installed copies check it before they install.          |
| All      | `latest.json`, the update list                         | Installed copies read it to find the newest version.    |
| All      | `SHA256SUMS` and these release notes                   | Checking that a download is the one that was published. |

The Mac packages are built for both processors on an Apple Silicon GitHub runner. The
workflow compiles and packages them but does not launch them; see
[the native verification limits](acceptance.md) and the macOS checks in
[the first-release checklist](#the-first-release-checklist).

The Linux packages are built on Ubuntu 22.04 on purpose. It is the oldest supported base
that ships WebKitGTK 4.1, so the packages run on more distributions than a build from a
newer base would. Upgrading that runner image raises the oldest Linux the packages run on,
so it is a reviewed change.

## The update key

Installed copies of Quota update themselves, and the one thing that stops a stranger from
sending them a program is a signature. Every updater payload is signed with a private key,
and the matching public key is built into Quota (`plugins.updater.pubkey` in
[`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json)). A copy installs an update
only if its signature verifies against that public key.

This is **not** an operating-system code-signing certificate. It is a key pair made with
`tauri signer generate`, it costs nothing, and it is separate from anything Apple or
Microsoft issue. No Apple Developer ID and no Windows certificate are used.

### Where the key is, and putting it in GitHub (once)

The pair was generated on the maintainer's machine into `~/.config/quota-updater/`, a
folder only that user can open, outside every repository:

| File                    | What it is                                                      |
| ----------------------- | --------------------------------------------------------------- |
| `quota-updater.key`     | The **private key**. Secret. Whoever has it can ship an update. |
| `key-password.txt`      | The password that unlocks the private key. Secret.              |
| `quota-updater.key.pub` | The public key, which is already in `tauri.conf.json`.          |

The release workflow reads the private key and its password from two GitHub **repository
secrets** with exactly these names:

- `TAURI_SIGNING_PRIVATE_KEY`: the whole contents of `quota-updater.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the contents of `key-password.txt`

With the GitHub command line, signed in as the repository owner, this puts each file in
its secret without printing it:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo ShayGus/quota < ~/.config/quota-updater/quota-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo ShayGus/quota < ~/.config/quota-updater/key-password.txt
```

Or in the browser: the repository's **Settings**, then **Secrets and variables**, then
**Actions**, then **New repository secret**, once for each name, pasting the file's
contents as the value. If the private key is absent, the first build step stops with a
message naming `TAURI_SIGNING_PRIVATE_KEY`. The password must unlock that key; a missing
or incorrect password prevents signing an encrypted key.

The two secrets are given to the build steps and to no other step: not the preflight, not
the tests, not the assembly, not the draft. `cargo xtask check-release` fails if the
workflow ever hands them anywhere else.

### Back it up, offline, now

**If the private key or its password is lost, no installed copy of Quota can ever be
updated again.** Installed copies trust only the public key they were built with. The only
recovery is to publish a version with a new key and ask everyone to download it by hand.

So, before the first release is published:

1. Copy `quota-updater.key` and `key-password.txt` to **two** places that are not this
   computer and not online: an encrypted USB stick, or a password manager's secure
   attachment, and a second copy elsewhere.
2. Do not put either file in a repository, a chat, an email, or an unencrypted cloud
   folder. Do not paste them into an issue or a pull request.
3. Keep the password and the key apart when you can.

If you believe the private key has leaked, say so before the next release: a leaked key
lets someone ship an update that installed copies accept.

## How an installed copy updates

The [Updates section of the README](../README.md#updates) owns the check schedule and
pop-up behaviour. The feed address is configured in `plugins.updater.endpoints` in
[`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json):

```
https://github.com/ShayGus/quota/releases/latest/download/latest.json
```

GitHub serves only **published** releases there. A draft release is never offered, so the
draft you build is invisible to every installed copy until you press **Publish release**.
That is what "an update is published" means.

The address and the key cannot be changed by an environment variable or a setting, and
`cargo xtask check-release` fails if a release build could be pointed anywhere else.

A copy updates with the kind of package it was installed from, so each has its own entry
in the list: the Windows installer and the MSI, the AppImage, the deb and the rpm, and the
Mac archive for each processor.

## Which tests a release needs

The release workflow refuses to start unless **every run of the normal CI workflow
([`ci.yml`](../.github/workflows/ci.yml)) on that exact commit completed successfully**.
That file owns the current jobs and commands; [CONTRIBUTING](../CONTRIBUTING.md#3-run-the-checks)
explains how to run the checks, including the
[interface suite](../CONTRIBUTING.md#7-interface-tests) and
[real-app suite](../CONTRIBUTING.md#8-real-app-tests-linux-and-windows).

Every job in `ci.yml`, including both real-app jobs, is covered without editing the
release workflow, because the preflight asks about the whole workflow.
The release workflow then runs the release gate, the architecture gate, the binding check
and `cargo test --workspace` again on the exact commit, with no secret present, and builds
only after those pass.

## 1. Prepare the version

The Tauri configuration is the authority: `version` in
[`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) decides what the application
reports and what the installers are named. Two files mirror it and
`cargo xtask check-release` fails when they disagree:

- `version` in [`package.json`](../package.json)
- `version` in the `[package]` table of [`src-tauri/Cargo.toml`](../src-tauri/Cargo.toml)

Change all three in one reviewed pull request, together with anything else the release
should contain, and let the normal CI pass. That commit is what gets released. The version
must go **up** from the release before it: installed copies only update to a higher
number.

Use three numbers, such as `1.2.0`. A version with letters in it, such as `1.2.0-rc.1`, is
refused, because the Windows MSI package version cannot hold it.

## 2. Cut the release

1. Open the **Actions** tab, choose **Release**, and choose **Run workflow**.
2. Pick `main` as the branch. The version comes from the branch you release.
3. Enter the version, for example `0.1.0`.
4. Press **Run workflow**.

The run refuses to continue, and creates nothing, when:

- the branch is not `main`;
- the version is not three numbers;
- the version is not the one in `tauri.conf.json`;
- the tag `v<version>` already exists, because a published release is never replaced;
- the normal CI has not completed successfully on that commit, or has never run on it.
  Every CI job counts, as in [Which tests a release needs](#which-tests-a-release-needs).
  One failing job stops the release;
- the release gate, the architecture gate, the binding check, or the tests fail;
- the update key is not in the repository secrets;
- any build fails: Windows, Linux, or either Mac. The draft is created only after all of
  them succeed;
- the update list does not verify. A separate job builds `latest.json` from the real
  signatures and refuses the release unless every platform (Windows, Linux, and both Macs)
  is listed, each signature is the one on disk and verifies its package under the public
  key in the repository, each address is an asset of this same release, and both the list's
  version and the signature's signed `version:` field match `tauri.conf.json`. A missing
  or mismatched signed version fails verification, matching the installed updater's
  `requireSignedVersion` requirement. The check is `cargo xtask update-manifest verify`,
  and its tests run in CI.

## 3. Review the draft

The draft appears on the **Releases** page, marked as a draft, with the tag pointing at
the reviewed commit. Download the packages and try them before anything is public:

- On a clean Windows machine, run the `.exe` and install it. It must ask for no
  administrator rights. Then install the `.msi` the same way, if you ship it.
- On a clean Linux machine, run the `.AppImage` and open Quota.
- Install the `.deb` and the `.rpm` on a machine of each kind and confirm the same.
- On a Mac, open the `.dmg` for its processor, drag Quota to Applications, and open it.
  macOS will not open it at once, because the app is not signed by Apple; see
  [Opening Quota on a Mac](#opening-quota-on-a-mac).
- With the previous version already installed, install this one over it. Your accounts,
  preferences, and history must still be there.
- Check every download against the checksums:

  ```bash
  sha256sum --check SHA256SUMS
  ```

The installers are not signed by Microsoft or Apple, so Windows shows an "unknown
publisher" warning and macOS asks for confirmation. That is expected for now; see the last
section.

### Opening Quota on a Mac

The Mac app is signed ad hoc, which makes it internally consistent but gives it no
identity Apple knows, and it is not notarised, because no Apple Developer ID is used. The
first time a person opens a downloaded copy, macOS refuses. They open **System Settings**,
then **Privacy & Security**, scroll to the message about Quota, and choose **Open
Anyway**; or they run `xattr -dr com.apple.quarantine /Applications/Quota.app` once. An
update that Quota installs for itself does not need this again, because the app downloads
it, not a browser.

## 4. Publish

1. Open the draft on the **Releases** page.
2. Fix the notes if you want to say more.
3. Press **Publish release**.

Publishing makes the tag and the packages public and exposes this release's `latest.json`
to installed copies at their next check; see [the update schedule](../README.md#updates).
The GitHub Actions run and the release are two separate objects; publishing does not
re-run anything.

## 5. Roll back

**Before publishing**, nothing is public yet and no installed copy can see it. Delete the
draft, fix the problem, and run the workflow again with the same version. Rebuilding a
draft is the normal correction.

**After publishing**, an installed copy cannot be downgraded, and one that has already
updated stays on the new version. The only way out is a higher version:

1. Stop the spread. Open the bad release, choose **Edit release**, and either tick **Set
   as a pre-release** or move **latest** back: tick **Set as a latest release** on the
   older release and choose **Update release**. Installed copies that have not updated yet
   then see the older, lower list and are not offered the bad version. Copies that did
   update keep it.
2. Fix the fault in `main`.
3. Cut the next, higher version as above.

## The first release checklist

CI checks the update logic; the release workflow verifies the payload signatures before
creating the draft. A real installation and update on each system still need manual
verification. Do this once, in order. Every step says **what you should see**; anything
else is a failure, and you should stop and report what you saw instead.

1. **Set up the key.** Put the two secrets in GitHub and back the key up offline, as in
   [The update key](#the-update-key). _You should see:_ both names under **Settings,
   Secrets and variables, Actions**.
2. **Publish v0.1.0.** Run the Release workflow for `0.1.0`. _You should see:_ every job
   green, and a **draft** release `v0.1.0` holding the installers, the `.sig` files,
   `latest.json`, and `SHA256SUMS`. Install the draft's packages on each system you can
   reach, covering Windows, Linux, and both Mac processors before release, as in
   [Review the draft](#3-review-the-draft). If a Mac build, a disk image, or a Mac launch
   fails, fix it before going on. Then press **Publish release**.
3. **Install v0.1.0** on the machine you will test the update on, from the published
   release, and start it. _You should see:_ Quota open as usual and **no pop-up**, because
   nothing newer exists.
4. **Make v0.1.1.** In a pull request, raise the version to `0.1.1` in the three files
   (section 1), merge it, let CI pass, and run the Release workflow for `0.1.1`. It stays
   a **draft**. With it unpublished, quit and start the installed v0.1.0 again. _You
   should see:_ **no pop-up.** A draft is never offered.
5. **Publish v0.1.1.** Press **Publish release**.
6. **Start v0.1.0** again (or wait for the next check under
   [the update schedule](../README.md#updates)). _You should see:_ one small Quota window,
   in Quota's own design, titled **Update available**, with the text "Quota 0.1.1 is
   available (you have 0.1.0). Install it now? Quota will restart." and two buttons,
   **Cancel** and **OK**. No window of the operating system's own style.
7. **Press Cancel** the first time. _You should see:_ the window closes, Quota keeps
   running as version 0.1.0, and that version is not offered again until Quota is
   restarted. Then restart it and press **OK**.
8. **Press OK.** _You should see:_ the same window changes to **Installing the update**
   with both buttons greyed out; then Quota closes and starts again by itself. Open Quota,
   **Settings** shows version **0.1.1** in the bottom-left corner of the navigation rail,
   and your accounts and preferences are all still there. No pop-up appears this time.
9. Repeat steps 3 to 8 for Windows (NSIS and MSI), Linux (AppImage, deb and rpm), and
   macOS (Apple Silicon and Intel). On Windows the installer runs in a passive window and
   Quota comes back by itself; the AppImage replaces itself in place; a deb or rpm install
   should ask for administrator rights through the system's prompt; a Mac copy updates
   itself without the **Open Anyway** step.

If the pop-up never appears when it should, the cause is almost always one of these: the
release is still a draft, the installed copy is a development build, the version in
`tauri.conf.json` was not raised, or the machine is offline. The log line "the update
check failed" (or "update checks are off in this build") says which.

## What this does not do yet

Each of these is a later phase, and none of them is needed to cut a release today.

- **No operating-system code signing.** No Windows certificate and no Apple Developer ID
  is used, and nothing is notarised. Windows shows an "unknown publisher" warning, macOS
  asks for confirmation on first open, and some Linux distributions warn about unsigned
  packages. Signing is a separate change that adds no other change to this workflow.
- **No package managers.** winget, Scoop, and Homebrew do not carry Quota yet. They take a
  published, stable release first.
- **No automatic publishing.** Every release waits for a person to publish it.
- **No update settings.** There is no switch to turn updates off, no "skip this version",
  no release notes in the pop-up, and no update channels (beta or stable).
- **The key cannot be rotated gracefully.** See [The update key](#the-update-key).
