import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const portalRoot = fileURLToPath(new URL('..', import.meta.url));
const schemaPath = fileURLToPath(new URL('../../../openapi/openapi.json', import.meta.url));
const generatedPath = fileURLToPath(new URL('../src/api/schema.d.ts', import.meta.url));
const temporaryDirectory = mkdtempSync(join(tmpdir(), 'hiqnet-portal-openapi-'));
const temporary = join(temporaryDirectory, 'schema.d.ts');
const cli = resolve(portalRoot, 'node_modules/openapi-typescript/bin/cli.js');
const result = spawnSync(process.execPath, [cli, schemaPath, '-o', temporary], {
  cwd: portalRoot,
  stdio: 'inherit',
});

try {
  if (result.status !== 0) process.exit(result.status ?? 1);
  const generated = readFileSync(temporary, 'utf8').replace(/\r\n/g, '\n').trimEnd();
  const current = readFileSync(generatedPath, 'utf8').replace(/\r\n/g, '\n').trimEnd();
  if (generated !== current) {
    console.error('El cliente OpenAPI está desactualizado. Ejecuta `pnpm api:generate`.');
    process.exit(1);
  }
  console.log('Cliente OpenAPI actualizado.');
} finally {
  rmSync(temporaryDirectory, { recursive: true, force: true });
}
