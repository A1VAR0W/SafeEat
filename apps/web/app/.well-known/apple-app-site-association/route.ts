export const dynamic = 'force-dynamic';
export function GET() {
  const team = process.env.APPLE_TEAM_ID;
  return Response.json(
    {
      applinks: {
        apps: [],
        details: team ? [{ appID: `${team}.com.a1var0w.safeeat`, paths: ['/p/*', '/r/*'] }] : [],
      },
    },
    { headers: { 'Cache-Control': 'public,max-age=3600' } },
  );
}
