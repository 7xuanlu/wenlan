# Code signing and notarization

What ships today, and what it takes to make the installers trusted by the OS.

| Artifact | Today | What the OS shows | Fix |
| --- | --- | --- | --- |
| `Wenlan_<ver>_aarch64.dmg`, `Wenlan_aarch64.app.tar.gz` | Developer ID signed and notarized, ticket stapled, from v0.17.4 | None. The first-run gauntlet on v0.17.4 reports `spctl --assess` as `accepted`, `source=Notarized Developer ID` | Done |
| `Wenlan_<ver>_x64-setup.exe` | unsigned; the READMEs disclose this beside the download | SmartScreen may warn about an unknown publisher | Windows signing is not active; see section 2 |
| `*.sig` next to the app bundles | signed with the Tauri updater key (`TAURI_SIGNING_PRIVATE_KEY`) | nothing; this is what the in-app updater verifies | already done; unrelated to Gatekeeper and SmartScreen |
| CLI tarballs, `wenlan-windows-x64.zip` | unsigned; `install.sh` strips quarantine and verifies `SHA256SUMS` | none for a terminal install | not needed for launch |

The Tauri updater signature protects updates after the first install. Gatekeeper and SmartScreen judge the first install. A Developer ID certificate plus notarization satisfies Gatekeeper outright; SmartScreen also weighs how often an installer has been downloaded, so a certificate quiets it only as downloads accumulate.

## 1. macOS: Developer ID and notarization

The workflow is already wired, and the secrets are in place: releases from v0.17.4 are signed and notarized. If the secrets are ever removed, the build falls back to ad-hoc signing and the verify step says so.

### One-time setup (about an hour, plus Apple's review of the membership)

1. **Join the Apple Developer Program** at <https://developer.apple.com/programs/enroll/> (USD 99 per year). After enrollment, note the ten-character **Team ID** under Membership details.
2. **Create a "Developer ID Application" certificate.** On any Mac: Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority, save the request to disk. Then at <https://developer.apple.com/account/resources/certificates/add> pick **Developer ID Application**, upload the request, download the `.cer`, and double-click it so it lands in the login keychain. Check it:

   ```bash
   security find-identity -v -p codesigning
   # 1) ABC123…  "Developer ID Application: Your Name (TEAMID)"
   ```

   The quoted string is the signing identity you will paste into a secret.
3. **Export the certificate with its private key.** Keychain Access → My Certificates → right-click the Developer ID Application entry → Export → `.p12`, choose a password. Encode it:

   ```bash
   base64 -i DeveloperID.p12 | tr -d '\n' > DeveloperID.p12.b64
   ```

   The value must be a single line; the workflow strips stray whitespace from `APPLE_CERTIFICATE` but rejects any other line break.

4. **Create an app-specific password for notarization.** <https://account.apple.com/account/manage> → Sign-In and Security → App-Specific Passwords → generate one named `wenlan notarization`.
5. **Add six repository secrets** (Settings → Secrets and variables → Actions, or `gh secret set NAME < file`, e.g. `gh secret set APPLE_CERTIFICATE < DeveloperID.p12.b64`):

   | Secret | Value |
   | --- | --- |
   | `APPLE_CERTIFICATE` | contents of `DeveloperID.p12.b64` (one line) |
   | `APPLE_CERTIFICATE_PASSWORD` | the `.p12` export password |
   | `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` |
   | `APPLE_ID` | the Apple ID email of the developer account |
   | `APPLE_PASSWORD` | the app-specific password from step 4 |
   | `APPLE_TEAM_ID` | the Team ID from step 1 |

   These are the names the Tauri CLI reads (`APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD` import the certificate into a temporary keychain on the runner; `APPLE_SIGNING_IDENTITY` overrides `signingIdentity` in `tauri.conf.json`; `APPLE_ID` with `APPLE_PASSWORD` and `APPLE_TEAM_ID` reach only the steps that talk to the notary service, never `pnpm tauri build` — see the outermost-container note below).
6. **Cut a release** as usual (`docs/RELEASING.md`). In the `app-bundle` job of `release.yml` the steps run in Apple's prescribed order:
   1. *Load Apple signing and notarization secrets* exports the three signing secrets and deliberately withholds the three notarization ones.
   2. `pnpm tauri build` signs the app and the bundled Wenlan binaries (`wenlan`, `wenlan-server`, `wenlan-mcp`) with the hardened runtime and `app/Entitlements.plist`, signs the disk image, and — because it cannot see the notarization secrets — submits nothing. The native reverse relay does not require a `cloudflared` sidecar.
   3. *Notarize and staple the DMG* makes the single submission and staples the ticket to the disk image.
   4. *Staple the app and rebuild the updater archive* staples the same ticket to the `.app` (a lookup, not a second submission), rebuilds `Wenlan_aarch64.app.tar.gz` from the stapled app, and re-signs it with `tauri signer sign`.
   5. *Verify Apple signature and notarization* fails the job unless `codesign --verify --deep --strict` passes and the signing authority is a Developer ID Application certificate, and — when the notarization secrets are set — `spctl --assess --type execute` and `xcrun stapler validate` pass on the `.app`, `stapler validate` plus `spctl --assess --type open` pass on the `.dmg`, and the app mounted from inside the disk image assesses as `source=Notarized Developer ID`.
7. **Prove it from a user's seat.** Run the first-run gauntlet on the new tag (`gh workflow run first-run-gauntlet.yml --ref main -f release_tag=vX.Y.Z -f channels=macos-app`). Its macOS leg stamps the quarantine attribute a browser would set, then requires `dmg-stapled`, `dmg-gatekeeper`, `dmg-developer-id`, `dmg-codesign-valid`, and `app-gatekeeper` (which must report `source=Notarized Developer ID`) to pass. Then delete the "ad-hoc signed and not notarized" paragraph from the four READMEs (`README.md`, `README.es-ES.md`, `README.zh-Hans.md`, `README.zh-Hant.md`) and close F11 in the gauntlet report. Done for v0.17.4: run 33287491599 passed all five checks, with both Gatekeeper legs reporting `accepted`, `source=Notarized Developer ID`, `origin=Developer ID Application: Qi-Xuan Lu (TDFFZXRF3D)`.

### Things to know

- Notarization waits on Apple's queue once, and the wait is unpredictable. Two throwaway bundles submitted at 07:04 and 07:21 UTC on 2026-08-29 were still `In Progress` eleven hours later, while Apple's system status page reported the Notary Service healthy; the developer forums describe this for first-time accounts, whose early submissions get held for review. The step waits 90 minutes inside a job that allows 210, and the workspace compiles from scratch before it, so the two numbers move together — raise both rather than assume a hang. If it does time out, the step prints the submission id — wait on that id with `xcrun notarytool wait <id>` instead of rerunning the job, because a rerun submits the same disk image again and queues behind the first.
- The entitlements `allow-jit` and `allow-unsigned-executable-memory` are ordinary hardened-runtime exceptions and pass notarization.
- A Developer ID certificate is valid for five years; apps notarized before it expires keep opening after.
- Keep the `.p12` and its password out of the repository and out of chat; the secrets are the only copy the workflow needs.
- **Only the outermost container is notarized.** One submission of the disk image also covers the app inside it. [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow): *"if you submit a disk image that contains a signed installer package with an app bundle inside, the notarization service generates tickets for the disk image, installer package, and app bundle."* The disk image is what a browser hands a user, quarantine attribute and all, so that is the container we submit. tauri-bundler would submit the `.app` on its own as soon as it sees `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` in the environment ([`macos/app.rs`](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle/macos/app.rs) at the pinned CLI 2.11.4), so *Load Apple signing and notarization secrets* keeps those three out of the job environment and hands them only to the steps that need them. Without them the bundler logs `skipping app notarization` and carries on. Do not add them back to the build step's environment: that silently restores a second submission and a second wait.
- **The app copied out of the disk image has no ticket of its own.** One submission covers it — a notarization ticket is a list of code directory hashes, and the app inside the image is in that list — but the copy inside the image was made before any ticket existed, so nothing is embedded in it. Gatekeeper resolves it against Apple on first launch, which needs a moment of network. Everything a person keeps is stapled: the disk image they downloaded, and the app the updater installs.
- **The updater archive is rebuilt, not the one Tauri produced.** A `.tar.gz` cannot hold a stapled ticket. Apple, on the equivalent ZIP case: *"While you can notarize a ZIP archive, you can't staple to it directly. Instead, run `stapler` against each item that you added to the archive. Then create a new ZIP file containing the stapled items for distribution."* Tauri writes `Wenlan_aarch64.app.tar.gz` before any ticket exists, so *Staple the app and rebuild the updater archive* staples the app, repacks it, and re-signs with `tauri signer sign`, which calls the same `sign_file` helper the bundler uses. The step unpacks the result and fails the job unless the app inside verifies and validates as stapled — a broken updater archive is the worst thing this job could ship.
- Notarization removes Gatekeeper's block, not every dialog. A quarantined app still shows the one-time "downloaded from the Internet, are you sure you want to open it?" confirmation, which has an Open button. What goes away is the "cannot be opened because Apple cannot check it for malicious software" refusal and the trip to System Settings to approve it. The README one-liner installer strips the quarantine attribute, so that path shows nothing at all.
- Developer ID signing without notarization still fails Gatekeeper, so set all six secrets together.

## 2. Windows code signing

Windows installers are not code-signed. No certificate provider is active for the published Windows installer.

The [v0.18.16 Windows release job](https://github.com/7xuanlu/wenlan/actions/runs/37169138734/job/111338400326) skipped the prepared signing and signature-verification steps. The [release notes](https://github.com/7xuanlu/wenlan/releases/tag/v0.18.16) disclose the unsigned installer. Prepared workflow steps are not evidence of a signed release.

The `.sig` distributed with an installer is for the Tauri updater. It is not a Windows Authenticode signature and does not establish a verified Windows publisher.

If Windows signing is enabled later, verify the final distributed installer with `Get-AuthenticodeSignature`, require a valid signature from the intended publisher, and regenerate updater signatures and checksums after the installer bytes change. Update the four READMEs only after that verification succeeds.

Until then, keep the unsigned status visible beside the Windows download. A checksum verifies the downloaded bytes, not a publisher identity.

## 3. Related integrity checks already in place

- `SHA256SUMS` is published on every release by the `finalize-release` job, and `install.sh` verifies the tarball it downloads against it.
- `scripts/install-macos-app.sh` (the README one-liner for the app) verifies the DMG against the SHA-256 digest GitHub records for the asset.
- The Tauri updater verifies every update with the minisign public key in `app/tauri.conf.json`.
- Homebrew and npm packages carry their own registry checksums.
