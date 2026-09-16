#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(node -p "require('./package.json').version")"
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
(cd "$package" && ditto -c -k --sequesterRsrc --keepParent Payload "$root/artifacts/SafeEat-$version.ipa")
cd "$root/artifacts"
test -s "SafeEat-$version.ipa"
unzip -tq "SafeEat-$version.ipa"
unzip -Z1 "SafeEat-$version.ipa" > "$package/entries.txt"
grep -Fxq 'Payload/SafeEat.app/Info.plist' "$package/entries.txt"
shasum -a 256 "SafeEat-$version.ipa" > "SafeEat-$version.ipa.sha256"
