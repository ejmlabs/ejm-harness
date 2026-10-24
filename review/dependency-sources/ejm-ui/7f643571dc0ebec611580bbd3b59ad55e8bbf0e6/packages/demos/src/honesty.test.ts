// @vitest-environment node
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';

const script = join(dirname(fileURLToPath(import.meta.url)), '../scripts/check-demos-honesty.mjs');
const dirs: string[] = [];
afterEach(() => dirs.splice(0).forEach((d) => rmSync(d, { recursive: true, force: true })));

/** Product checkouts in a temp dir, each with the README text given. */
function checkouts(readmes: Record<string, string>, files: Record<string, string> = {}) {
  const root = mkdtempSync(join(tmpdir(), 'honesty-'));
  dirs.push(root);
  for (const p of ['ejm-cli', 'ejm-ide', 'ejm-harness']) {
    mkdirSync(join(root, p), { recursive: true });
    writeFileSync(join(root, p, 'README.md'), readmes[p] ?? '# empty\n');
  }
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}
const run = (root: string) => {
  const r = spawnSync(process.execPath, [script, '--root', root], { encoding: 'utf8' });
  return { code: r.status, out: r.stdout };
};

describe('honesty gate', () => {
  it('fails and names exactly what a README does not document', () => {
    const { code, out } = run(checkouts({ 'ejm-cli': 'Use /help. Enter sends. Ctrl+C stops. Escape closes. Add `--allow-write`.' }));
    expect(code).toBe(1);
    expect(out).toContain('MISSING command "/new"');
    expect(out).toContain('MISSING flag "--plain"');
    expect(out).not.toContain('MISSING command "/help"');
    expect(out).not.toContain('MISSING key "Enter"');
    expect(out).not.toContain('MISSING key "Esc"');
  });

  it('does not take a plain word in prose for a tool', () => {
    const { out } = run(checkouts({ 'ejm-ide': 'You can search the code, read files and list files.' }));
    const ide = out.slice(out.indexOf('ejm-ide\n'), out.indexOf('ejm-harness\n'));
    expect(ide).toContain('MISSING tool "workbench_search_code"');
  });

  it('accepts a tool name only when code formatted, in the README or in the file the allow list cites', () => {
    const tools = 'Tools: `workbench_search_code`, `workbench_read_file`, `workbench_write_file`, `workbench_open_file`.';
    const inReadme = run(checkouts({ 'ejm-ide': tools })).out;
    // The allow entries are stale once the README documents the names, which is how the gate asks for them to be removed.
    expect(inReadme).toContain('stale allow entry');
    const viaAllow = run(checkouts({ 'ejm-ide': 'No tools here.' }, { 'ejm-ide/docs/agent-tools.md': tools })).out;
    expect(viaAllow.slice(viaAllow.indexOf('ejm-ide\n'), viaAllow.indexOf('ejm-harness\n'))).toContain('0 not documented');
    const prose = run(checkouts({ 'ejm-ide': 'No tools here.' }, { 'ejm-ide/docs/agent-tools.md': 'search code, read file, write file' })).out;
    expect(prose).toContain('does not contain it');
  });

  it('reports a missing checkout instead of passing', () => {
    const root = checkouts({});
    rmSync(join(root, 'ejm-ide'), { recursive: true });
    const { code, out } = run(root);
    expect(code).toBe(1);
    expect(out).toMatch(/FAIL .*ejm-ide\/README\.md does not exist/);
  });
});
