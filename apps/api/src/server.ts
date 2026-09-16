import { createApp } from './app';
import { loadEnv } from './config/env';
const env = loadEnv();
const app = await createApp(env);
await app.listen({ host: '0.0.0.0', port: env.PORT });
for (const signal of ['SIGINT', 'SIGTERM'] as const)
  process.on(signal, () => {
    void app.close().then(() => process.exit(0));
  });
