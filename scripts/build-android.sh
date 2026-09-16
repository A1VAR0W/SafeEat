#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(node -p "require('./package.json').version")"
mkdir -p artifacts
if [[ "${PRODUCTION_SIGNING:-false}" == true ]]; then
  : "${ANDROID_KEYSTORE_BASE64:?required}" "${ANDROID_KEYSTORE_PASSWORD:?required}" "${ANDROID_KEY_ALIAS:?required}" "${ANDROID_KEY_PASSWORD:?required}"
  export SAFEEAT_KEYSTORE_PATH="${RUNNER_TEMP:?}/safeeat-release.jks"
  printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 --decode > "$SAFEEAT_KEYSTORE_PATH"
  chmod 600 "$SAFEEAT_KEYSTORE_PATH"
  trap 'rm -f "$SAFEEAT_KEYSTORE_PATH"' EXIT
  node scripts/configure-android-signing.mjs
fi
cd apps/mobile/android
chmod +x gradlew
./gradlew assembleRelease bundleRelease --no-daemon --max-workers=2 -PreactNativeArchitectures=arm64-v8a,armeabi-v7a,x86_64 > "${RUNNER_TEMP:-/tmp}/safeeat-gradle.log" 2>&1 || { tail -120 "${RUNNER_TEMP:-/tmp}/safeeat-gradle.log"; exit 1; }
cd "$root"
cp apps/mobile/android/app/build/outputs/apk/release/app-release.apk "artifacts/SafeEat-$version.apk"
cp apps/mobile/android/app/build/outputs/bundle/release/app-release.aab "artifacts/SafeEat-$version.aab"
tools="$(find "$ANDROID_HOME/build-tools" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
"$tools/apksigner" verify --verbose --print-certs "artifacts/SafeEat-$version.apk" > artifacts/SafeEat-signing.txt
"$tools/aapt" dump badging "artifacts/SafeEat-$version.apk" > artifacts/SafeEat-badging.txt
grep -Fq "package: name='com.a1var0w.safeeat'" artifacts/SafeEat-badging.txt
grep -Fq "versionName='$version'" artifacts/SafeEat-badging.txt
grep -Fq "versionCode='${BUILD_NUMBER:-1}'" artifacts/SafeEat-badging.txt
unzip -tq "artifacts/SafeEat-$version.apk"
unzip -tq "artifacts/SafeEat-$version.aab"
cd artifacts
sha256sum "SafeEat-$version.apk" > "SafeEat-$version.apk.sha256"
sha256sum "SafeEat-$version.aab" > "SafeEat-$version.aab.sha256"
