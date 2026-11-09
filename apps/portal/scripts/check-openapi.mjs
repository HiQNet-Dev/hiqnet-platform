import { execFileSync } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const portalRoot = fileURLToPath(new URL('..', import.meta.url));
const schemaPath = fileURLToPath(new URL('../../../openapi/openapi.json', import.meta.url));
const outputUrl = new URL('../src/api/schema.d.ts', import.meta.url);
const temporary = path.join(os.tmpdir(), `hiqnet-openapi-${process.pid}.d.ts`);

try {
  execFileSync('pnpm', ['exec', 'openapi-typescript', schemaPath, '-o', temporary], {
    cwd: portalRoot,
    stdio: 'inherit',
  });
  const generated = (await fs.readFile(temporary, 'utf8')).trimEnd();
  const current = (await fs.readFile(outputUrl, 'utf8')).trimEnd();
  if (generated !== current) {
    console.error('El cliente OpenAPI está desactualizado. Ejecuta `pnpm api:generate`.');
    process.exit(1);
  }
  console.log('Cliente OpenAPI actualizado.');
} finally {
  await fs.rm(temporary, { force: true });
}
