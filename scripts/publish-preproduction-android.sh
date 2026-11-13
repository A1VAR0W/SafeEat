#!/usr/bin/env bash
# Publishes a SafeEat 2.x preproduction APK as an in-app update (Android).
#
#   bash scripts/publish-preproduction-android.sh SafeEat-2.0.4-preproduction.apk "Notas en texto plano"
#
# 1. Reads package, versionName and versionCode from the APK itself (aapt).
# 2. Uploads it to /var/www/safeeat-downloads/SafeEat-<version>-preproduction.apk and
#    checks the SHA-256 on the server.
# 3. Only then writes preproduction-android.json (the manifest preproduction builds
#    read), keeping a backup of the previous one. Production's releases/latest.json
#    is never touched.
set -euo pipefail

apk="${1:?APK path}"
notes="${2:-}"
host="${SAFEEAT_HOST:-chatink-dev}"
downloads=/var/www/safeeat-downloads
public=https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads

sdk="${ANDROID_HOME:-${LOCALAPPDATA:-}/Android/Sdk}"
aapt="$(ls -d "$sdk"/build-tools/*/ | sort -V | tail -1)aapt"
[[ -x "$aapt" || -x "$aapt.exe" ]] || { echo "aapt not found under $sdk/build-tools" >&2; exit 1; }

badging="$("$aapt" dump badging "$apk" | head -1)"
package="$(sed -n "s/.*package: name='\([^']*\)'.*/\1/p" <<<"$badging")"
version="$(sed -n "s/.*versionName='\([^']*\)'.*/\1/p" <<<"$badging")"
code="$(sed -n "s/.*versionCode='\([0-9]*\)'.*/\1/p" <<<"$badging")"
[[ "$package" == com.a1var0w.safeeat ]] || { echo "not SafeEat: $package" >&2; exit 1; }
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "$code" =~ ^[0-9]+$ ]] || { echo "bad version: $badging" >&2; exit 1; }

name="SafeEat-$version-preproduction.apk"
sha="$(sha256sum "$apk" | cut -d' ' -f1)"
size="$(stat -c %s "$apk")"
echo "SafeEat $version (versionCode $code), $size bytes, sha256 $sha"

scp -q "$apk" "$host:/tmp/$name"
ssh "$host" "sudo install -m 644 -o root -g root /tmp/$name $downloads/$name && rm -f /tmp/$name &&
  test \"\$(sha256sum $downloads/$name | cut -d' ' -f1)\" = $sha"
echo "APK published: $public/$name"

manifest="$(mktemp)"
python -I - "$manifest" "$version" "$code" "$size" "$sha" "$public/$name" "$name" "$notes" <<'EOF'
import json, sys
out, version, code, size, sha, url, name, notes = sys.argv[1:]
json.dump({
    "schemaVersion": 1,
    "release": {"tag_name": f"v{version}", "body": notes},
    "android": {"name": name, "url": url, "size": int(size), "sha256": sha, "versionCode": int(code)},
}, open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
EOF
scp -q "$manifest" "$host:/tmp/preproduction-android.json"
rm -f "$manifest"
ssh "$host" "cd $downloads && { [ ! -f preproduction-android.json ] || sudo cp -p preproduction-android.json preproduction-android.json.bak-\$(date +%Y%m%d%H%M%S); } &&
  sudo install -m 644 -o root -g root /tmp/preproduction-android.json preproduction-android.json && rm -f /tmp/preproduction-android.json"

# What the app will read (no cache in between).
curl -sfI "$public/preproduction-android.json?t=$(date +%s)" | grep -iE '^(HTTP|cache-control|content-type)' || true
curl -sf "$public/preproduction-android.json?t=$(date +%s)" | python -I -c "import json,sys; d=json.load(sys.stdin); print('manifest:', d['release']['tag_name'], d['android']['versionCode'])"
