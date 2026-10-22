#!/usr/bin/env node
// Protect the public brand-download inputs against package file-list exclusions.
import { execFileSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkg = dirname(dirname(fileURLToPath(import.meta.url)));
const [pack] = JSON.parse(execFileSync('npm', ['pack', '--dry-run', '--json'], { cwd: pkg, encoding: 'utf8' }));
const packed = new Set(pack.files.map(file => file.path));
function files(path) {
  return readdirSync(path, { withFileTypes: true }).flatMap(entry => {
    const file = join(path, entry.name);
    return entry.isDirectory() ? files(file) : [relative(pkg, file).replaceAll('\\', '/')];
  });
}
const expected = files(join(pkg, 'assets')).filter(file => !file.endsWith('.zip') && !file.endsWith('/.DS_Store'));
const missing = expected.filter(file => !packed.has(file));
if (missing.length) throw new Error(`Public brand inputs missing from package:\n${missing.join('\n')}`);
console.log(`brand-pack: ${expected.length} public asset files included; ZIP assembly stays with Studio`);
