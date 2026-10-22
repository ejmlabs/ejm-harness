// @vitest-environment node
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const src = fileURLToPath(new URL('..', import.meta.url));
const files = ['views', 'islands', 'hero', 'previews'].flatMap((d) => readdirSync(join(src, d)).filter((f) => /\.(tsx?|ts)$/.test(f) && !/\.test\./.test(f)).map((f) => join(src, d, f)));
files.push(join(src, 'cli-playback.ts'), join(src, 'harness-playback.ts'), join(src, 'recordings/playback.ts'));

describe('views, islands, hero and playback adapters', () => {
  it('use no network, storage, cookie or tracking API', () => {
    const banned = /\bfetch\s*\(|XMLHttpRequest|sendBeacon|WebSocket|EventSource|localStorage|sessionStorage|indexedDB|document\.cookie|navigator\.(geolocation|clipboard)/;
    for (const f of files) expect(readFileSync(f, 'utf8'), f).not.toMatch(banned);
  });
});
