# Releasing

A release is one button on GitHub. You give it a version, it builds Windows and Linux
packages, and it puts them in a **draft release**. Nothing is public until you press
Publish. This page is the whole procedure.

The workflow is [`.github/workflows/release.yml`](../.github/workflows/release.yml). It
uses no secret beyond the automatic `GITHUB_TOKEN`, and only the last job can write.

## What a release contains

| Platform | Package                                                | Who it is for                                           |
| -------- | ------------------------------------------------------ | ------------------------------------------------------- |
| Windows  | Installer `.exe` (NSIS), installs for the current user | Everyone. No administrator rights needed.               |
| Windows  | `.msi`                                                 | Companies that deploy software centrally.               |
| Linux    | `.AppImage`, runs without installing                   | Everyone.                                               |
| Linux    | `.deb`                                                 | Ubuntu and Debian.                                      |
| Linux    | `.rpm`                                                 | Fedora and openSUSE.                                    |
| Both     | `SHA256SUMS` and these release notes                   | Checking that a download is the one that was published. |

The Linux packages are built on Ubuntu 22.04 on purpose. It is the oldest supported base
that ships WebKitGTK 4.1, so the packages run on more distributions than a build from a
newer base would. Upgrading that runner image raises the oldest Linux the packages run on,
so it is a reviewed change.

## 1. Prepare the version

The Tauri configuration is the authority: `version` in
[`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) decides what the application
reports and what the installers are named. Two files mirror it and
`cargo xtask check-release` fails when they disagree:

- `version` in [`package.json`](../package.json)
- `version` in the `[package]` table of [`src-tauri/Cargo.toml`](../src-tauri/Cargo.toml)

Change all three in one reviewed pull request, together with anything else the release
should contain, and let the normal CI pass. That commit is what gets released.

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
  Every CI job counts: the Rust unit tests, the Windows job, the frontend, the interface
  tests, and the real-app tests. One failing job stops the release;
- the release gate, the architecture gate, the binding check, or the tests fail;
- either build fails. The draft is created only after both succeed.

## 3. Review the draft

The draft appears on the **Releases** page, marked as a draft, with the tag pointing at
the reviewed commit. Download the packages and try them before anything is public:

- On a clean Windows machine, run the `.exe` and install it. It must ask for no
  administrator rights. Then install the `.msi` the same way, if you ship it.
- On a clean Linux machine, run the `.AppImage` and open Quota.
- Install the `.deb` and the `.rpm` on a machine of each kind and confirm the same.
- With the previous version already installed, install this one over it. Your accounts,
  preferences, and history must still be there.
- Check every download against the checksums:

  ```bash
  sha256sum --check SHA256SUMS
  ```

The installers are not signed, so Windows shows an "unknown publisher" warning. That is
expected for now; see the last section.

## 4. Publish

1. Open the draft on the **Releases** page.
2. Fix the notes if you want to say more.
3. Press **Publish release**.

Publishing makes the tag and the packages public. The GitHub Actions run and the release
are two separate objects; publishing does not re-run anything.

## 5. Roll back

**Before publishing**, nothing is public yet. Delete the draft, fix the problem, and run
the workflow again with the same version. Rebuilding a draft is the normal correction.

**After publishing**, an installed copy cannot be downgraded. The only way out is a higher
version:

1. Make the previous release the latest one, so a new install gets the version you trust.
   Open that release, choose **Edit release**, tick **Set as a latest release**, pick the
   older version in the list, and choose **Update release**. The newer release stays
   published; it is only no longer the latest.
2. Fix the fault in `main`.
3. Cut the next, higher version as above.

## What this does not do yet

Each of these is a later phase, and none of them is needed to cut a release today.

- **No in-app updates.** Quota does not check for a new version by itself. The AppImage
  and the installer are the update path until an update key and an update list exist.
- **No code signing.** No certificate is used. Windows shows an "unknown publisher"
  warning, and some Linux distributions warn about unsigned packages. Signing is a
  separate change that adds no other change to this workflow.
- **No macOS build.** It is not built and not tested, so it is not released. See
  [Platforms](platforms.md).
- **No package managers.** winget, Scoop, and Homebrew do not carry Quota yet. They take a
  published, stable release first.
- **No automatic publishing.** Every release waits for a person to publish it.
