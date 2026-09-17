#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(node -p "require('./package.json').version")"
artifact="SafeEat-$version-${APP_ENV:?required}"
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
cp apps/mobile/android/app/build/outputs/apk/release/app-release.apk "artifacts/$artifact.apk"
cp apps/mobile/android/app/build/outputs/bundle/release/app-release.aab "artifacts/$artifact.aab"
tools="$(find "$ANDROID_HOME/build-tools" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
"$tools/apksigner" verify --verbose --print-certs "artifacts/$artifact.apk" > artifacts/SafeEat-signing.txt
"$tools/aapt" dump badging "artifacts/$artifact.apk" > artifacts/SafeEat-badging.txt
grep -Fq "package: name='com.a1var0w.safeeat'" artifacts/SafeEat-badging.txt
grep -Fq "versionName='$version'" artifacts/SafeEat-badging.txt
grep -Fq "versionCode='${BUILD_NUMBER:-1}'" artifacts/SafeEat-badging.txt
grep -Fq "android.permission.INTERNET" artifacts/SafeEat-badging.txt
if grep -Fq 'application-debuggable' artifacts/SafeEat-badging.txt; then exit 1; fi
python3 scripts/check-embedded-config.py "artifacts/$artifact.apk"
unzip -tq "artifacts/$artifact.apk"
unzip -tq "artifacts/$artifact.aab"
cd artifacts
sha256sum "$artifact.apk" > "$artifact.apk.sha256"
sha256sum "$artifact.aab" > "$artifact.aab.sha256"
