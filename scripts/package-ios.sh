#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(node -p "require('./package.json').version")"
artifact="SafeEat-$version-${APP_ENV:?required}"
app="$root/build/DerivedData/Build/Products/Release-iphoneos/SafeEat.app"
test -d "$app"
plist="$app/Info.plist"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" = com.a1var0w.safeeat
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")" = "$version"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$plist")" = "${BUILD_NUMBER:-1}"
package="$(mktemp -d)"
trap 'rm -rf "$package"' EXIT
mkdir -p "$package/Payload" "$root/artifacts"
ditto "$app" "$package/Payload/SafeEat.app"
(cd "$package" && ditto -c -k --sequesterRsrc --keepParent Payload "$root/artifacts/$artifact.ipa")
cd "$root/artifacts"
python3 "$root/scripts/check-embedded-config.py" "$artifact.ipa"
test -s "$artifact.ipa"
unzip -tq "$artifact.ipa"
unzip -Z1 "$artifact.ipa" > "$package/entries.txt"
grep -Fxq 'Payload/SafeEat.app/Info.plist' "$package/entries.txt"
shasum -a 256 "$artifact.ipa" > "$artifact.ipa.sha256"
