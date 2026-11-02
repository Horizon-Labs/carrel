# Releasing

How a release reaches each device. The Mac app and the command line are installed directly; the iPad and iPhone app goes through TestFlight.

## TestFlight (iPad and iPhone)

The App Store Connect record is `io.github.hannaharose.Carrel`, team `4V63YRQU43`, with the owner as an internal tester. The Mac app isn't uploaded: a Mac TestFlight build needs App Sandbox, which Carrel's Mac app doesn't use.

From the repository root, on a Mac with Xcode signed in to the team's account (Xcode → Settings → Accounts):

```bash
scripts/build-xcframework.sh
```

```bash
scripts/testflight.sh --dry-run
```

`--dry-run` archives the app for generic iOS and exports it signed for App Store Connect, to `target/testflight/export/Carrel.ipa`, and uploads nothing. Without a flag the script does the same and then asks before uploading; `--upload` uploads without asking. Signing is automatic, through the Xcode account and `-allowProvisioningUpdates`, which registers the app's identifiers (the app, `.Share` and the app group `group.io.github.hannaharose.Carrel`) the first time. The script never takes a password; for CI, an App Store Connect API key can stand in for the Xcode account, through `ASC_KEY_PATH`, `ASC_KEY_ID` and `ASC_ISSUER_ID`.

- **Version:** the workspace version in `Cargo.toml`, without its pre-release part, since Apple takes only numbers: 1.0.0-rc.1 is uploaded as 1.0.0.
- **Build number:** `git rev-list --count HEAD`, so it rises with each commit merged. Upload from `dev` or `main`: a branch's count can be higher than a later `dev`'s, and App Store Connect refuses a build number it has already seen for that version.
- **What an upload needs, and where it is:** the privacy manifests (`apple/CarrelApp/Carrel/PrivacyInfo.xcprivacy` and `CarrelShare/PrivacyInfo.xcprivacy`: no tracking, no data collected, and the reasons for the required-reason APIs used), `ITSAppUsesNonExemptEncryption` set to no (Carrel uses only the system's HTTPS; Kopia isn't on iOS), the launch screen and iPad orientations, and the usage strings, all in `apple/CarrelApp/Carrel-iOS-Info.plist`. The icon comes from `AppIcon.icon`, which Xcode turns into every iOS size, including the 1024-point marketing icon.
- **When adding a capability to the iOS app:** add its usage string to `Carrel-iOS-Info.plist` (the camera, the photo library if Carrel ever reads it directly rather than through the picker, the microphone, the local network), and a required-reason API it calls to the privacy manifest. App Store Connect rejects an upload that's missing either.

After an upload, App Store Connect processes the build for a few minutes before TestFlight offers it; answer the export-compliance question there only if the encryption key above changes.
