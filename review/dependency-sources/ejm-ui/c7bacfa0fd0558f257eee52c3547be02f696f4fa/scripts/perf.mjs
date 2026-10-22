#!/usr/bin/env node
// Performance gate for a built site: transfer sizes, FCP, LCP, CLS and blocking time per route, measured in headless Chrome
// on a throttled mobile profile, against the budgets in QUALITY.md. See scripts/PERF.md for what the numbers mean and how noisy they are.
//
//   node scripts/perf.mjs <dist-dir | url...> [--routes /,/lazy/] [--ignore glob]... [--budget-file f.json] [--runs 3] [--json]
//                         [--lcp 1200] [--cls 0.05] [--tolerance 15]
//
// A dist directory is served from a small static server (gzip on text, like a CDN; `/route/` -> `/route/index.html`; 404.html for
// the rest). Every .html file is a route unless it matches an --ignore glob (or `ignore` in .ejm-ssg/build.json), as in `ejm-ssg budget`. Urls are measured as they are. Exit code: 0 within budget, 1 on a breach, 2 on a usage or Chrome error.
// No dependencies: Chrome is driven over the DevTools Protocol with Node's global WebSocket (set CHROME to use another binary).
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';
import { DEFAULT_BUDGET, isIgnored } from '../packages/ssg/src/budget.mjs';

const KB = 1024;
export const TARGETS = { lcp: 1200, cls: 0.05, tolerance: 15 }; // ms, score, percent (applied to LCP only)
// Mobile profile: 375x812 at DPR 3, 4x CPU slowdown, "Slow 4G" (1.6 Mbps down, 750 kbps up, 150 ms round trip).
export const PROFILE = { width: 375, height: 812, dpr: 3, cpu: 4, latency: 150, down: (1.6 * 1024 * 1024) / 8, up: (750 * 1024) / 8 };
const SETTLE_MS = 1500; // wait after the load event so late paints and layout shifts are counted

// ---- Pure parts ----

export const median = nums => {
  const v = nums.filter(n => Number.isFinite(n)).sort((a, b) => a - b);
  if (!v.length) return null;
  const mid = v.length >> 1;
  return v.length % 2 ? v[mid] : (v[mid - 1] + v[mid]) / 2;
};

const GROUP = { Document: 'html', Stylesheet: 'css', Script: 'js', Font: 'fonts' };

/** Collects Network.* DevTools events (fake ones in tests) into transfer bytes per group. encodedDataLength includes response headers. */
export function createTally() {
  const reqs = new Map();
  return {
    push(method, p) {
      if (method === 'Network.requestWillBeSent') {
        const r = reqs.get(p.requestId);
        if (!r) reqs.set(p.requestId, { url: p.request.url, type: p.type, bytes: 0, done: false });
        else r.url = p.request.url; // a redirect reuses the id
      } else if (method === 'Network.responseReceived') {
        const r = reqs.get(p.requestId);
        if (r && p.type) r.type = p.type;
      } else if (method === 'Network.loadingFinished') {
        const r = reqs.get(p.requestId);
        if (r) Object.assign(r, { bytes: p.encodedDataLength, done: true });
      }
    },
    summary() {
      const bytes = { html: 0, css: 0, js: 0, fonts: 0, other: 0 };
      let requests = 0;
      for (const r of reqs.values()) {
        if (!r.done || /^(data|blob|about):|\/favicon\.ico$/.test(r.url)) continue; // Chrome asks for a favicon on its own
        requests++;
        bytes[GROUP[r.type] ?? 'other'] += r.bytes;
      }
      return { bytes, requests };
    },
  };
}

/** Collapses several runs of one route into medians. Each run: { bytes, requests, fcp, lcp, cls, tbt, longTasks }. */
export function medianRun(runs) {
  const m = key => median(runs.map(r => r[key]));
  return {
    bytes: Object.fromEntries(['html', 'css', 'js', 'fonts', 'other'].map(k => [k, median(runs.map(r => r.bytes[k]))])),
    requests: m('requests'), fcp: m('fcp'), lcp: m('lcp'), cls: m('cls'), tbt: m('tbt'), longTasks: m('longTasks'),
  };
}

/** Budget numbers (KB) from a JSON file: either { js, css, fonts, html } or { budget: { ... } } (the shape of .ejm-ssg/build.json). */
export function readBudgetFile(file) {
  const j = JSON.parse(readFileSync(file, 'utf8'));
  return j.budget ?? j;
}

/** Breaches for rows [{ route, ...medianRun, noJs }]. Sizes: KB limits. LCP: target plus tolerance percent. CLS: strictly under the target. */
export function evaluate(rows, { budget = {}, lcp = TARGETS.lcp, cls = TARGETS.cls, tolerance = TARGETS.tolerance } = {}) {
  const limits = { ...DEFAULT_BUDGET, ...budget };
  const kb = n => (n / KB).toFixed(1);
  const breaches = [];
  for (const r of rows) {
    for (const k of ['html', 'css', 'js', 'fonts']) {
      const over = r.bytes[k] - limits[k] * KB;
      if (over > 0) breaches.push(`${r.route}: ${k} is ${kb(r.bytes[k])} KB transferred, over the ${limits[k]} KB budget by ${kb(over)} KB`);
    }
    const ceiling = lcp * (1 + tolerance / 100);
    if (r.lcp == null) breaches.push(`${r.route}: LCP was not measured`);
    else if (r.lcp > ceiling) breaches.push(`${r.route}: LCP is ${Math.round(r.lcp)} ms, over the ${lcp} ms target (+${tolerance}% tolerance = ${Math.round(ceiling)} ms) by ${Math.round(r.lcp - ceiling)} ms`);
    if (r.cls == null) breaches.push(`${r.route}: CLS was not measured`);
    else if (r.cls >= cls) breaches.push(`${r.route}: CLS is ${r.cls.toFixed(3)}, not under ${cls} (over by ${(r.cls - cls).toFixed(3)})`);
  }
  return { breaches, limits };
}

const TYPES = { '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8', '.json': 'application/json', '.webmanifest': 'application/manifest+json', '.svg': 'image/svg+xml', '.xml': 'application/xml', '.txt': 'text/plain; charset=utf-8', '.map': 'application/json', '.woff2': 'font/woff2', '.woff': 'font/woff', '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.webp': 'image/webp', '.avif': 'image/avif', '.gif': 'image/gif', '.ico': 'image/x-icon' };
const COMPRESSIBLE = /^(text\/|application\/(json|xml|manifest)|image\/svg)/;

/** Maps a request url to { status, file }. Directories serve their index.html; unknown paths get 404.html (or no file). */
export function resolveRoute(dist, rawUrl) {
  const root = resolve(dist);
  const notFound = existsSync(join(root, '404.html')) ? join(root, '404.html') : null;
  let target;
  try { target = resolve(root, '.' + decodeURIComponent(new URL(rawUrl, 'http://x').pathname)); } catch { return { status: 404, file: notFound }; }
  if (target !== root && !target.startsWith(root + sep)) return { status: 404, file: notFound };
  if (existsSync(target) && statSync(target).isDirectory()) target = join(target, 'index.html');
  return existsSync(target) && statSync(target).isFile() ? { status: 200, file: target } : { status: 404, file: notFound };
}

/** Serves `dist` on a free local port. Text is gzipped (level 9, like the budget gate) when the client accepts it. Resolves { url, close }. */
export function serve(dist) {
  const cache = new Map();
  const server = createServer((req, res) => {
    const { status, file } = resolveRoute(dist, req.url ?? '/');
    if (!file) return void res.writeHead(status, { 'content-type': 'text/plain' }).end('not found');
    const type = TYPES[extname(file)] ?? 'application/octet-stream';
    let body = readFileSync(file);
    const headers = { 'content-type': type, 'cache-control': 'no-store' };
    if (COMPRESSIBLE.test(type) && /\bgzip\b/.test(req.headers['accept-encoding'] ?? '')) {
      if (!cache.has(file)) cache.set(file, gzipSync(body, { level: 9 }));
      body = cache.get(file);
      headers['content-encoding'] = 'gzip';
    }
    res.writeHead(status, { ...headers, 'content-length': body.length }).end(req.method === 'HEAD' ? undefined : body);
  });
  return new Promise(res => server.listen(0, '127.0.0.1', () => res({ url: `http://127.0.0.1:${server.address().port}`, close: () => new Promise(r => { server.close(r); server.closeAllConnections(); }) })));
}

/** Route paths of every prerendered page under dist: '/', '/lazy/', '/404.html'. Pages matching an `ignore` glob (see `ejm-ssg budget`) are left out. */
export function discoverRoutes(dist, ignore = []) {
  const walk = d => readdirSync(d).flatMap(n => (statSync(join(d, n)).isDirectory() ? walk(join(d, n)) : [join(d, n)]));
  return walk(dist).filter(f => f.endsWith('.html')).map(f => relative(dist, f).split(sep).join('/'))
    .map(rel => ({ rel, route: rel === 'index.html' ? '/' : '/' + rel.replace(/index\.html$/, '') }))
    .filter(({ rel, route }) => !isIgnored(ignore, rel, route)).map(({ route }) => route).sort();
}

const bytesKb = n => (n / KB).toFixed(1);
const ms = n => (n == null ? 'n/a' : String(Math.round(n)));

export function formatReport(rows, { limits, lcp, cls, tolerance, breaches, profile = PROFILE }) {
  const mark = (over, text) => text + (over ? '!' : ' ');
  const head = ['route', 'js', 'css', 'fonts', 'html', 'reqs', 'FCP', 'LCP', 'CLS', 'TBT', 'LCP/noJS', 'CLS/noJS'];
  const body = rows.map(r => [
    r.route,
    ...['js', 'css', 'fonts', 'html'].map(k => mark(r.bytes[k] > limits[k] * KB, bytesKb(r.bytes[k]))),
    String(r.requests), ms(r.fcp),
    ms(r.lcp) + (r.lcp == null || r.lcp > lcp * (1 + tolerance / 100) ? '!' : r.lcp > lcp ? '~' : ' '),
    mark(r.cls == null || r.cls >= cls, r.cls == null ? 'n/a' : r.cls.toFixed(3)), ms(r.tbt),
    ms(r.noJs?.lcp), r.noJs?.cls == null ? 'n/a' : r.noJs.cls.toFixed(3),
  ]);
  const w = head.map((h, i) => Math.max(h.length, ...body.map(b => b[i].length)) + 1);
  const line = c => c.map((x, i) => (i ? x.padStart(w[i]) : x.padEnd(w[i]))).join('  ');
  return [
    `ejm perf: ${profile.width}x${profile.height}@${profile.dpr}, CPU ${profile.cpu}x slower, Slow 4G (${(profile.down * 8 / 1024 / 1024).toFixed(1)} Mbps, ${profile.latency} ms RTT); sizes are KB transferred, times ms (medians)`,
    `limits: js ${limits.js}, css ${limits.css}, fonts ${limits.fonts}, html ${limits.html} KB; LCP ${lcp} ms (+${tolerance}% tolerance; ~ = over target, inside tolerance); CLS under ${cls}`,
    line(head), ...body.map(line),
    breaches.length ? `\nOVER BUDGET\n  ${breaches.join('\n  ')}` : '\nall routes within budget',
  ].join('\n');
}

// ---- Chrome and the DevTools Protocol ----

export function findChrome() {
  return [process.env.CHROME, '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser'].find(p => p && existsSync(p)) ?? null;
}

let chrome = null, profileDir = '';
function cleanup() {
  if (chrome && chrome.exitCode === null) { try { chrome.kill('SIGKILL'); } catch {} }
  if (profileDir) { try { rmSync(profileDir, { recursive: true, force: true }); } catch {} profileDir = ''; }
}
process.on('exit', cleanup);
for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, () => { cleanup(); process.exit(130); });

async function launch(bin) {
  profileDir = mkdtempSync(join(tmpdir(), 'ejm-perf-'));
  chrome = spawn(bin, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profileDir}`, '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-background-networking', '--hide-scrollbars', '--mute-audio', '--force-color-profile=srgb', '--disable-gpu', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
  return await new Promise((resolve, reject) => {
    let tail = Buffer.alloc(0), settled = false;
    const failure = message => new Error(`${message}\nChrome stderr (last 8 KiB):\n${tail.length ? tail.toString('utf8').trimEnd() : '(no output)'}`);
    const finish = (error, url) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      chrome.removeListener('error', onError);
      chrome.removeListener('close', onClose);
      chrome.stderr.removeListener('data', onData);
      chrome.stderr.resume();
      error ? reject(error) : resolve(url);
    };
    const onError = error => finish(failure(`cannot start Chrome at ${bin}: ${error.message}`));
    const onClose = (code, signal) => finish(failure(`Chrome exited early (code ${code}, signal ${signal ?? 'none'})`));
    const onData = data => {
      const output = Buffer.concat([tail, data]);
      const match = output.toString('utf8').match(/DevTools listening on (ws:\/\/\S+)/);
      tail = Buffer.from(output.subarray(-8192));
      if (match) finish(null, match[1]);
    };
    const timer = setTimeout(() => finish(failure('Chrome did not report a debugging port in 60 s')), 60000);
    chrome.once('error', onError);
    chrome.once('close', onClose);
    chrome.stderr.on('data', onData);
  });
}

class Cdp {
  constructor(ws) {
    this.ws = ws; this.id = 0; this.pending = new Map(); this.listeners = new Set();
    ws.addEventListener('message', ev => {
      const msg = JSON.parse(ev.data);
      if (msg.id) {
        const p = this.pending.get(msg.id);
        if (!p) return;
        this.pending.delete(msg.id);
        msg.error ? p.reject(new Error(`${p.method}: ${msg.error.message}`)) : p.resolve(msg.result);
      } else if (msg.method) for (const l of this.listeners) l(msg.method, msg.params, msg.sessionId);
    });
  }
  static async connect(url) {
    const ws = new WebSocket(url);
    await new Promise((res, rej) => { ws.addEventListener('open', res, { once: true }); ws.addEventListener('error', () => rej(new Error('websocket failed')), { once: true }); });
    return new Cdp(ws);
  }
  on(fn) { this.listeners.add(fn); return () => this.listeners.delete(fn); }
  send(method, params = {}, session, timeout = 30000) {
    const id = ++this.id;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`${method} timed out after ${timeout} ms`)); }, timeout);
      this.pending.set(id, { method, resolve: v => { clearTimeout(timer); resolve(v); }, reject: e => { clearTimeout(timer); reject(e); } });
      this.ws.send(JSON.stringify({ id, method, params, ...(session ? { sessionId: session } : {}) }));
    });
  }
}

// Installed before any page script (JS-on runs only): long tasks for total blocking time. Everything else comes from the browser side.
const LONG_TASKS = `window.__ejmLong = [];
new PerformanceObserver(l => { for (const e of l.getEntries()) window.__ejmLong.push([e.startTime, e.duration]); }).observe({ type: 'longtask', buffered: true });`;

/** One cold load of `url` in a fresh browser context. Returns { bytes, requests, fcp, lcp, cls, tbt, longTasks }; tbt and longTasks are null with JS off. */
async function measureOnce(cdp, url, { js }) {
  const { browserContextId } = await cdp.send('Target.createBrowserContext');
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank', browserContextId });
  const { sessionId: s } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  const tally = createTally();
  const state = { navStart: null, wall: null, fcp: null, lcpWall: null, shifts: 0, loaded: false };
  const off = cdp.on((method, p, session) => {
    if (session !== s) return;
    if (method.startsWith('Network.')) tally.push(method, p);
    if (method === 'Network.requestWillBeSent' && p.type === 'Document' && state.navStart == null) Object.assign(state, { navStart: p.timestamp, wall: p.wallTime });
    else if (method === 'Page.lifecycleEvent' && p.name === 'firstContentfulPaint' && state.fcp == null) state.fcp = p.timestamp;
    else if (method === 'Page.loadEventFired') state.loaded = true;
    else if (method === 'PerformanceTimeline.timelineEventAdded') {
      const e = p.event;
      if (e.type === 'largest-contentful-paint' && e.lcpDetails) state.lcpWall = e.lcpDetails.renderTime || e.lcpDetails.loadTime;
      else if (e.type === 'layout-shift' && e.layoutShiftDetails && !e.layoutShiftDetails.hadRecentInput) state.shifts += e.layoutShiftDetails.value;
    }
  });
  try {
    for (const [m, params] of [
      ['Page.enable', {}], ['Network.enable', {}], ['Page.setLifecycleEventsEnabled', { enabled: true }],
      ['PerformanceTimeline.enable', { eventTypes: ['largest-contentful-paint', 'layout-shift'] }],
      ['Emulation.setDeviceMetricsOverride', { width: PROFILE.width, height: PROFILE.height, deviceScaleFactor: PROFILE.dpr, mobile: true }],
      ['Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 }],
      ['Network.setCacheDisabled', { cacheDisabled: true }],
      ['Network.emulateNetworkConditions', { offline: false, latency: PROFILE.latency, downloadThroughput: PROFILE.down, uploadThroughput: PROFILE.up }],
      ['Emulation.setCPUThrottlingRate', { rate: PROFILE.cpu }],
      ['Emulation.setScriptExecutionDisabled', { value: !js }],
    ]) await cdp.send(m, params, s);
    if (js) await cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: LONG_TASKS }, s);
    await cdp.send('Page.navigate', { url }, s);
    const t0 = Date.now();
    while (!state.loaded) {
      if (Date.now() - t0 > 60000) throw new Error(`${url} did not finish loading in 60 s`);
      await new Promise(r => setTimeout(r, 50));
    }
    await new Promise(r => setTimeout(r, SETTLE_MS));
    const rel = t => (t == null || state.navStart == null ? null : (t - state.navStart) * 1000);
    let tbt = null, longTasks = null;
    if (js) {
      const r = await cdp.send('Runtime.evaluate', { expression: 'window.__ejmLong || []', returnByValue: true }, s);
      const fcp = rel(state.fcp) ?? 0;
      const tasks = (r.result.value ?? []).filter(([start, dur]) => start + dur > fcp); // blocking time counts after first paint
      tbt = tasks.reduce((n, [, d]) => n + Math.max(0, d - 50), 0);
      longTasks = tasks.length;
    }
    return { ...tally.summary(), fcp: rel(state.fcp), lcp: state.lcpWall != null && state.wall != null ? (state.lcpWall - state.wall) * 1000 : null, cls: state.shifts, tbt, longTasks };
  } finally {
    off();
    await cdp.send('Target.closeTarget', { targetId }).catch(() => {});
    await cdp.send('Target.disposeBrowserContext', { browserContextId }).catch(() => {});
  }
}

/** Measures each route `runs` times with JS on and `runs` times with JS off. Returns rows with medians and a `noJs` block. */
export async function measureRoutes(cdp, base, routes, runs) {
  const rows = [];
  for (const route of routes) {
    const url = new URL(route, base).href;
    const on = [], off = [];
    for (let i = 0; i < runs; i++) on.push(await measureOnce(cdp, url, { js: true }));
    for (let i = 0; i < runs; i++) off.push(await measureOnce(cdp, url, { js: false }));
    const noJs = medianRun(off);
    rows.push({ route, ...medianRun(on), noJs: { fcp: noJs.fcp, lcp: noJs.lcp, cls: noJs.cls } });
  }
  return rows;
}

// ---- CLI ----

function parseArgs(argv) {
  const o = { inputs: [], routes: null, ignore: [], budgetFile: null, runs: 3, json: false, ...TARGETS };
  const num = (flag, v) => { const n = Number(v); if (!Number.isFinite(n) || n < 0) throw new Error(`${flag} needs a number`); return n; };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') o.json = true;
    else if (a === '--routes') o.routes = String(argv[++i] ?? '').split(',').filter(Boolean);
    else if (a === '--ignore') o.ignore.push(String(argv[++i] ?? ''));
    else if (a === '--budget-file') o.budgetFile = argv[++i];
    else if (a === '--runs') o.runs = Math.max(1, Math.floor(num(a, argv[++i])));
    else if (a === '--lcp' || a === '--cls' || a === '--tolerance') o[a.slice(2)] = num(a, argv[++i]);
    else if (a.startsWith('--')) throw new Error(`unknown option ${a}`);
    else o.inputs.push(a);
  }
  if (!o.inputs.length) throw new Error('missing <dist-dir | url...>');
  return o;
}

const asRoute = r => (r.startsWith('/') ? r : '/' + r) + (/[./]$|\.[a-z0-9]+$/i.test(r) ? '' : '/');

export async function main(argv) {
  let o;
  try { o = parseArgs(argv); } catch (e) {
    console.error(`${e.message}\nUsage: node scripts/perf.mjs <dist-dir | url...> [--routes a,b] [--ignore glob]... [--budget-file f] [--runs 3] [--json] [--lcp 1200] [--cls 0.05] [--tolerance 15]`);
    return 2;
  }
  const bin = findChrome();
  if (!bin) return console.error('perf: Chrome not found. Install Google Chrome or set CHROME to a Chrome or Chromium binary.'), 2;
  const isDir = existsSync(o.inputs[0]) && statSync(o.inputs[0]).isDirectory();
  let server = null, targets, budget = {}, ignore = o.ignore;
  try {
    if (isDir) {
      const dist = resolve(o.inputs[0]);
      const saved = join(dirname(dist), '.ejm-ssg', 'build.json'); // the budget the site declared at build time, like `ejm-ssg budget`
      if (existsSync(saved)) {
        const config = JSON.parse(readFileSync(saved, 'utf8'));
        budget = config.budget ?? {};
        ignore = [...(config.ignore ?? []), ...o.ignore];
      }
      server = await serve(dist);
      targets = (o.routes ? o.routes.map(asRoute) : discoverRoutes(dist, ignore)).map(route => ({ route, base: server.url }));
    } else {
      const urls = o.inputs.map(u => new URL(u));
      targets = urls.map(u => ({ route: u.pathname + u.search, base: u.href }));
    }
    if (o.budgetFile) budget = readBudgetFile(o.budgetFile);
    const cdp = await Cdp.connect(await launch(bin));
    const rows = [];
    for (const t of targets) rows.push(...(await measureRoutes(cdp, t.base, [t.route], o.runs)).map(r => ({ ...r, route: t.route })));
    try { await cdp.send('Browser.close', {}, undefined, 3000); } catch {}
    const { breaches, limits } = evaluate(rows, { budget, lcp: o.lcp, cls: o.cls, tolerance: o.tolerance });
    console.log(o.json ? JSON.stringify({ profile: PROFILE, runs: o.runs, limits, targets: { lcp: o.lcp, cls: o.cls, tolerance: o.tolerance }, rows, breaches }, null, 2) : formatReport(rows, { limits, lcp: o.lcp, cls: o.cls, tolerance: o.tolerance, breaches }));
    return breaches.length ? 1 : 0;
  } catch (e) {
    console.error(`perf: ${e.message}`);
    return 2;
  } finally {
    await server?.close();
    cleanup();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) process.exitCode = await main(process.argv.slice(2));
