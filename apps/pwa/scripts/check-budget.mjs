// Verifica el presupuesto de rendimiento del bundle inicial.
//
// "Inicial" = HTML + entrada JS + todo lo que la entrada importa de forma
// estática + su CSS. Los imports dinámicos (rutas diferidas) no cuentan,
// porque no bloquean la primera pantalla.
//
// Se mide con gzip nivel 9 porque es la compresión que sirve nginx:alpine.
// Si se excede el presupuesto, el proceso termina con código 1 y el build falla.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';

const BUDGET_BYTES = 60 * 1024;
const dist = new URL('../dist/', import.meta.url).pathname;
const manifest = JSON.parse(readFileSync(join(dist, '.vite/manifest.json'), 'utf8'));

const entries = Object.entries(manifest).filter(([, chunk]) => chunk.isEntry);
if (entries.length === 0) {
  console.error('✗ No se encontró ningún chunk de entrada en el manifest de Vite.');
  process.exit(1);
}

const files = new Set(['index.html']);
const visit = (key) => {
  const chunk = manifest[key];
  if (!chunk || files.has(chunk.file)) return;
  files.add(chunk.file);
  for (const css of chunk.css ?? []) files.add(css);
  for (const imported of chunk.imports ?? []) visit(imported);
};
for (const [key] of entries) visit(key);

let total = 0;
const rows = [...files].map((file) => {
  const size = gzipSync(readFileSync(join(dist, file)), { level: 9 }).length;
  total += size;
  return { file, size };
});

const kb = (bytes) => `${(bytes / 1024).toFixed(1)} KB`;
for (const { file, size } of rows.sort((a, b) => b.size - a.size)) {
  console.log(`  ${kb(size).padStart(9)}  ${file}`);
}
const percent = ((total / BUDGET_BYTES) * 100).toFixed(0);
const summary = `Bundle inicial: ${kb(total)} gzip de ${kb(BUDGET_BYTES)} (${percent} %)`;

if (total > BUDGET_BYTES) {
  console.error(`✗ ${summary}. Se excede el presupuesto; el build falla.`);
  process.exit(1);
}
console.log(`✓ ${summary}`);
