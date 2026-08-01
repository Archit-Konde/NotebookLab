# Releasing NotebookLab

The release pipeline is automated end to end. A maintainer's job is three
commands; the workflow does the rest.

## Steps

1. **Bump the version** in all four places. They must match the tag, or the
   release workflow fails on purpose:

   - `package.json` (`version`)
   - `src-tauri/Cargo.toml` (`version`)
   - `src-tauri/tauri.conf.json` (`version`)
   - `src-tauri/Cargo.lock` (the `notebooklab` package entry)

   The lock file is checked because it was the one nobody remembered: it sat at
   0.7.7 through three releases without anything noticing.

2. **Update `CHANGELOG.md`** with a new section for the version, dated. The
   release badge in `README.md` reads the latest release from GitHub and needs
   no editing; it was maintained by hand only while the repository was private
   and a dynamic badge would have shown "repo not found".

3. **Commit, tag, and push**:

   ```bash
   git commit -am "Release v0.5.0"
   git tag v0.5.0
   git push && git push --tags
   ```

   (Substitute the version you are releasing.)

## What the workflow does

On a `v*` tag, `.github/workflows/release.yml`:

1. Verifies the tag matches the committed versions (fails fast if not).
2. Builds installers on four runners: Windows (`.msi`, `-setup.exe`),
   macOS Intel and Apple Silicon (`.dmg`), Linux (`.deb`, `.rpm`).
3. Signs update bundles with the Tauri updater key and generates
   `latest.json` (secrets `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).
4. Collects everything on a draft release.
5. After all platforms succeed: generates `SHA256SUMS`, uploads it, and
   publishes the release as latest.

## macOS signing and notarization

The workflow is fully wired for Apple code signing. It activates on the next
tag after the six `APPLE_*` secrets exist; until then macOS builds ship
unsigned and users need the one-time right-click Open documented in the
README. The setup is a one-time job for the account owner because it needs
an Apple account, a payment method, and repository admin rights:

1. **Enroll** at [developer.apple.com](https://developer.apple.com/programs/enroll/)
   in the Apple Developer Program (99 USD per year, identity verification
   included). Note your **Team ID** from the membership page.

2. **Create the certificate.** In Xcode (Settings, Accounts, Manage
   Certificates) or at
   [developer.apple.com/account/resources/certificates](https://developer.apple.com/account/resources/certificates/list),
   create a **Developer ID Application** certificate. Export it from Keychain
   Access as a `.p12` file with a strong password, then encode it:

   ```bash
   base64 -i DeveloperIDApplication.p12 | pbcopy
   ```

3. **Create an app-specific password** for notarization at
   [account.apple.com](https://account.apple.com/account/manage) under
   Sign-In and Security, App-Specific Passwords.

4. **Add six repository secrets** (Settings, Secrets and variables, Actions):

   | Secret | Value |
   |--------|-------|
   | `APPLE_CERTIFICATE` | the base64 string from step 2 |
   | `APPLE_CERTIFICATE_PASSWORD` | the `.p12` export password |
   | `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` |
   | `APPLE_ID` | the Apple account email |
   | `APPLE_PASSWORD` | the app-specific password from step 3 |
   | `APPLE_TEAM_ID` | the Team ID from step 1 |

5. **Tag the next release.** Signing and notarization run automatically;
   Gatekeeper opens the app first try, and the right-click note can come out
   of the README.

Publishing is what makes `releases/latest/download/latest.json` resolve, which
is the endpoint installed apps poll for auto-updates. If any platform build
fails, the draft stays unpublished and users see nothing half-finished.

## After the release

- Check the [releases page](https://github.com/Amey-Thakur/NotebookLab/releases)
  shows all assets: installers per platform, update bundles with `.sig` files,
  `latest.json`, and `SHA256SUMS`.
- Install the Windows or macOS build once and confirm it launches and shows
  the new version in Settings.
- Older installs pick up the update automatically on next launch.
