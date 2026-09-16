export const dynamic = 'force-dynamic';
export function GET() {
  const fingerprints = (process.env.ANDROID_SHA256_FINGERPRINTS ?? '')
    .split(',')
    .map((v) => v.trim())
    .filter((v) => /^([A-Fa-f0-9]{2}:){31}[A-Fa-f0-9]{2}$/.test(v));
  return Response.json(
    fingerprints.length
      ? [
          {
            relation: ['delegate_permission/common.handle_all_urls'],
            target: {
              namespace: 'android_app',
              package_name: 'com.a1var0w.safeeat',
              sha256_cert_fingerprints: fingerprints,
            },
          },
        ]
      : [],
    { headers: { 'Cache-Control': 'public,max-age=3600' } },
  );
}
