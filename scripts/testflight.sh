#!/usr/bin/env bash
# Archives the iPad and iPhone app, exports it signed for App Store Connect, and uploads it to
# TestFlight after asking (#493). The Mac app isn't uploaded: it's installed directly.
#
#   scripts/testflight.sh             archive, export, then ask before uploading
#   scripts/testflight.sh --dry-run   archive and export the .ipa only; nothing is uploaded
#   scripts/testflight.sh --upload    archive, export and upload without asking
#
# Build the release framework first: scripts/build-xcframework.sh.
#
# The version is the workspace's (Cargo.toml) without any pre-release part, since Apple takes
# only numbers: 1.0.0-rc.1 is uploaded as 1.0.0. The build number is the commit count of HEAD,
# so it rises with every commit merged; upload from dev or main.
#
# Signing is automatic, with team TEAM (4V63YRQU43 by default) through the Xcode account on
# this Mac; -allowProvisioningUpdates lets Xcode register the app's identifiers and profiles,
# and the iCloud container its entitlements name for the CloudKit writer lock. The script never
# takes a password. For CI, an App Store Connect API key can stand in for the
# Xcode account: ASC_KEY_PATH (the .p8 file), ASC_KEY_ID and ASC_ISSUER_ID.
set -euo pipefail

cd "$(dirname "$0")/.."

mode=ask
case "${1:-}" in
  "") ;;
  --dry-run) mode=dry-run ;;
  --upload) mode=upload ;;
  *)
    echo "usage: scripts/testflight.sh [--dry-run | --upload]" >&2
    exit 2
    ;;
esac

team="${TEAM:-4V63YRQU43}"
project=apple/CarrelApp/CarrelApp.xcodeproj
out=target/testflight
archive="$out/Carrel.xcarchive"

if [ ! -d apple/CarrelKit/CarrelFFI.xcframework ]; then
  echo "CarrelFFI.xcframework is missing. Build it first: scripts/build-xcframework.sh" >&2
  exit 1
fi

workspace_version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)"
version="${workspace_version%%-*}"
build="$(git rev-list --count HEAD)"
if [ -n "$(git status --porcelain)" ]; then
  echo "Note: the working tree has uncommitted changes; build $build won't match a commit." >&2
fi

auth=()
if [ -n "${ASC_KEY_PATH:-}" ]; then
  auth=(-authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "${ASC_KEY_ID:?}"
    -authenticationKeyIssuerID "${ASC_ISSUER_ID:?}")
fi

# Writes the export options for a destination: "export" leaves the .ipa here, "upload" sends
# it to App Store Connect.
export_options() {
  cat >"$out/ExportOptions-$1.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>method</key>
	<string>app-store-connect</string>
	<key>destination</key>
	<string>$1</string>
	<key>teamID</key>
	<string>$team</string>
	<key>signingStyle</key>
	<string>automatic</string>
	<key>manageAppVersionAndBuildNumber</key>
	<false/>
	<key>uploadSymbols</key>
	<true/>
</dict>
</plist>
EOF
  echo "$out/ExportOptions-$1.plist"
}

rm -rf "$out"
mkdir -p "$out"

echo "== Archiving Carrel $version ($build) for iOS ($workspace_version)"
xcodebuild archive -project "$project" -scheme Carrel -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$archive" \
  -allowProvisioningUpdates ${auth[@]+"${auth[@]}"} \
  DEVELOPMENT_TEAM="$team" MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$build" \
  CARREL_BUILD="$(git rev-parse --short=10 HEAD)" \
  -quiet

echo "== Exporting"
xcodebuild -exportArchive -archivePath "$archive" -exportPath "$out/export" \
  -exportOptionsPlist "$(export_options export)" -allowProvisioningUpdates ${auth[@]+"${auth[@]}"} -quiet
ipa="$out/export/Carrel.ipa"
echo "Exported $ipa"

case "$mode" in
  dry-run)
    echo "Dry run: nothing uploaded."
    exit 0
    ;;
  ask)
    read -r -p "Upload Carrel $version ($build) to TestFlight? [y/N] " answer
    case "$answer" in
      y | Y | yes) ;;
      *)
        echo "Not uploaded."
        exit 0
        ;;
    esac
    ;;
esac

echo "== Uploading"
xcodebuild -exportArchive -archivePath "$archive" -exportPath "$out/upload" \
  -exportOptionsPlist "$(export_options upload)" -allowProvisioningUpdates ${auth[@]+"${auth[@]}"}
echo "Uploaded Carrel $version ($build). It appears in TestFlight once App Store Connect has processed it."
