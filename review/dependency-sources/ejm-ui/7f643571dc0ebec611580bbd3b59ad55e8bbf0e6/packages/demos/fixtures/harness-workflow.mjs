/** Offline SDK workflow. Records real worker events and admission reservations. */
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const harnessDir = resolve(process.env.EJM_HARNESS_DIR ?? new URL('../../../../ejm-harness', import.meta.url).pathname);
const { Harness } = await import(`${harnessDir}/sdk/typescript/dist/index.js`);
const root = mkdtempSync(join(tmpdir(), 'ejm-sdk-showcase-'));
const state = join(root, 'state');
const workspaceA = join(root, 'harbor-a');
const workspaceB = join(root, 'harbor-b');
mkdirSync(workspaceA); mkdirSync(workspaceB);
writeFileSync(join(workspaceA, 'README.md'), '# Harbor\nA small order service.\n');
const responses = [
  { tool_calls: [{ id: 'read', name: 'read_file', arguments: { path: 'README.md' } }] },
  { tool_calls: [{ id: 'first', name: 'shell', arguments: { command: "sleep 2; printf 'harbor-a checks complete\\n'", heavy: true } }] },
  { tool_calls: [{ id: 'second', name: 'shell', arguments: { command: "printf 'harbor-b checks complete\\n'", heavy: true } }] },
  { content: 'The checks completed. The session journal keeps the tool results.' },
  { content: 'The second workspace completed its checks.' },
];
const options = { state_dir: state, allow_shell: true, provider: { kind: 'replay', responses }, resources: { max_heavy_jobs: 1 } };
let harness;
const log = [];
const print = text => { log.push(text); console.log(text); };
try {
  harness = await Harness.create(options);
  const a = await harness.createSession({ cwd: workspaceA, title: 'Harbor checks' });
  const b = await harness.createSession({ cwd: workspaceB, title: 'Independent workspace' });
  print('provider: replay (offline)');
  print('resources: max_heavy_jobs=1');
  const first = await a.start('Read the README and run the checks.');
  for (let n = 0; n < 100; n++) { if ((await harness.governor()).heavy_running === 1) break; await delay(10); }
  const second = await b.start('Run the checks in this workspace.');
  let busy;
  for (let n = 0; n < 100; n++) { busy = await harness.governor(); if (busy.queued > 0) break; await delay(10); }
  if (busy.running !== 1 || busy.queued !== 1 || busy.heavy_running !== 1) throw new Error(`No queued heavy job: ${JSON.stringify(busy)}`);
  print('governor while two workspaces request heavy work:');
  print(JSON.stringify(busy));
  print('harness while work is admitted:');
  print(JSON.stringify(await harness.status()));
  const results = await Promise.all([first.wait(), second.wait()]);
  const events = await a.events(0, 64);
  print('Harbor session journal:');
  for (const event of events) {
    const detail = event.type === 'tool_started' || event.type === 'tool_queued' ? event.call.name : event.type === 'tool_finished' ? event.output.content.replace(/\s+/g, ' ').trim() : event.type === 'text_delta' ? event.text : event.type === 'run_finished' ? event.summary.status : '';
    print(`${String(event.sequence).padStart(2)} ${event.type}${detail ? '  ' + detail : ''}`);
  }
  print(`runs: ${results.map(result => result.status).join(', ')}`);
  print('governor after completion:');
  print(JSON.stringify(await harness.governor()));
  const sessionId = a.id;
  await harness.close(); harness = undefined;
  harness = await Harness.create({ state_dir: state, provider: { kind: 'replay', responses: [] } });
  const reopened = (await harness.listSessions()).find(session => session.id === sessionId);
  if (!reopened) throw new Error('Saved session missing');
  const page = await reopened.events(0, 4);
  print(`reopened saved session: ${reopened.info.title}`);
  print(`events(after=0, limit=4): ${page.map(event => event.type).join(', ')}`);
  if (process.env.EJM_RECORD_SHOWCASE) writeFileSync(new URL('./harness-output.json', import.meta.url), JSON.stringify({ output: log, events, busy, results }, null, 2) + '\n');
} finally { await harness?.close(); rmSync(root, { recursive: true, force: true }); }
