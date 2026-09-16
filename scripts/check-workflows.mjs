import { readFileSync, readdirSync } from 'node:fs';
import YAML from 'yaml';
for (const name of readdirSync('.github/workflows')) {
  const doc = YAML.parse(readFileSync(`.github/workflows/${name}`, 'utf8'));
  if (!doc.name || !doc.on || !doc.jobs) throw new Error(`Invalid workflow: ${name}`);
  console.log(`Valid YAML: ${name}`);
}
