/** Record the commands shown by the CLI showcase against the authored Harbor fixture. */
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = mkdtempSync(join(tmpdir(), 'ejm-harbor-showcase-'));
try {
  cpSync(new URL('./harbor/', import.meta.url), root, { recursive: true });
  execFileSync('git', ['init', '-q'], { cwd: root });
  execFileSync('git', ['add', '.'], { cwd: root });
  execFileSync('git', ['-c', 'user.name=Demo fixture', '-c', 'user.email=demo@example.invalid', 'commit', '-qm', 'Harbor fixture'], { cwd: root });
  writeFileSync(join(root, 'src/retry.rs'), readFileSync(new URL('./retry.fixed.rs', import.meta.url)));
  const records = [];
  for (const command of ['cargo test --quiet', 'git diff --check', 'git diff -- src/retry.rs']) {
    const started = performance.now();
    const result = spawnSync('/bin/bash', ['--noprofile', '--norc', '-c', command], { cwd: root, encoding: 'utf8', env: { ...process.env, CARGO_TARGET_DIR: join(root, 'target') } });
    if (result.status !== 0) throw new Error(`${command}: ${result.stderr}`);
    records.push({ command, exit: result.status, durationMs: Math.round(performance.now() - started), stdout: result.stdout, stderr: result.stderr });
  }
  writeFileSync(new URL('./cli-output.json', import.meta.url), JSON.stringify({ fixture: 'harbor', records }, null, 2) + '\n');
  console.log(JSON.stringify(records, null, 2));
} finally { rmSync(root, { recursive: true, force: true }); }
