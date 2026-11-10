import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const appRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const directory = mkdtempSync(join(tmpdir(), "hiqnet-control-openapi-"));
const generated = join(directory, "openapi.ts");
const cli = resolve(appRoot, "node_modules/openapi-typescript/bin/cli.js");
const source = resolve(appRoot, "../../openapi/openapi.json");
const result = spawnSync(process.execPath, [cli, source, "-o", generated], { stdio: "inherit" });

try {
  if (result.status !== 0) process.exit(result.status ?? 1);
  const expected = readFileSync(resolve(appRoot, "src/api/generated/openapi.ts"), "utf8").replace(/\r\n/g, "\n");
  const actual = readFileSync(generated, "utf8").replace(/\r\n/g, "\n");
  if (expected !== actual) {
    console.error("OpenAPI generado desactualizado. Ejecuta: pnpm api:generate");
    process.exit(1);
  }
  console.log("OpenAPI generado sincronizado sin modificar el árbol de trabajo.");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
