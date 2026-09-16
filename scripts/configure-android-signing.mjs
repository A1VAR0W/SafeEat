import { readFileSync, writeFileSync } from 'node:fs';
const path = 'apps/mobile/android/app/build.gradle';
let text = readFileSync(path, 'utf8');
if (!text.includes('signingConfig signingConfigs.debug'))
  throw new Error('Unexpected Expo signing template');
text = text.replace(
  'signingConfigs {',
  `signingConfigs {
        production {
            storeFile file(System.getenv("SAFEEAT_KEYSTORE_PATH"))
            storePassword System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias System.getenv("ANDROID_KEY_ALIAS")
            keyPassword System.getenv("ANDROID_KEY_PASSWORD")
        }`,
);
// Keep the debug variant independent; change only the release signing entry.
const start = text.indexOf('release {', text.indexOf('buildTypes'));
if (start < 0) throw new Error('Missing release build type');
text =
  text.slice(0, start) +
  text
    .slice(start)
    .replace('signingConfig signingConfigs.debug', 'signingConfig signingConfigs.production');
writeFileSync(path, text);
