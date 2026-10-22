// @vitest-environment node
import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { expect, it } from 'vitest';
import { findChrome } from '../../../scripts/perf.mjs';

const chrome = findChrome();
it.skipIf(!chrome).each(['coarse', 'fine'])('keeps static controls correctly sized on %s pointers', async pointer => {
  const work = await mkdtemp(join(tmpdir(), 'ejm-static-touch-'));
  try {
    const css = await readFile(new URL('../public/static.css', import.meta.url), 'utf8');
    const html = join(work, 'index.html');
    await writeFile(html, `<style>${css}</style><button data-ejm="button" data-size="sm">Small</button><button data-ejm="button" data-density="compact">Compact</button><button data-ejm="button" data-density="compact" data-size="icon" aria-label="More">+</button><button data-ejm="button" data-size="lg">Large</button><pre id="result"></pre><script>document.querySelector('#result').textContent=JSON.stringify({coarse:matchMedia('(pointer:coarse)').matches,sizes:[...document.querySelectorAll('button')].map(b=>({width:b.getBoundingClientRect().width,height:b.getBoundingClientRect().height}))})</script>`);
    const { stdout } = await promisify(execFile)(chrome, ['--headless=new', '--disable-gpu', '--no-first-run', '--disable-background-networking', `--user-data-dir=${join(work, 'profile')}`, `--blink-settings=primaryPointerType=${pointer === 'coarse' ? 2 : 4},availablePointerTypes=${pointer === 'coarse' ? 2 : 4}`, '--dump-dom', `file://${html}`], { timeout: 15000, maxBuffer: 1024 * 1024 });
    const result = JSON.parse(stdout.match(/<pre id="result">([^<]+)<\/pre>/)?.[1] ?? 'null');
    expect(result.coarse).toBe(pointer === 'coarse');
    if (pointer === 'coarse') {
      for (const size of result.sizes) {
        expect(size.width).toBeGreaterThanOrEqual(44);
        expect(size.height).toBeGreaterThanOrEqual(44);
      }
    } else {
      expect(result.sizes[0].height).toBe(36);
      expect(result.sizes[1].height).toBe(32);
      expect(result.sizes[2]).toEqual({ width: 32, height: 32 });
    }
    expect(result.sizes[3].height).toBeGreaterThanOrEqual(56);
  } finally {
    await rm(work, { recursive: true, force: true });
  }
}, 20000);
