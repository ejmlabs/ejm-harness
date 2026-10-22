// @vitest-environment node
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { HarnessOutput } from './harness-parts';

it('formats recorded reservation values and preserves unknown or malformed output', () => {
  const known = { running: 1, queued: 1, heavy_running: 1, reserved_cpu_millis: 1000, reserved_memory_bytes: 67108864, reserved_output_bytes: 4096, reserved_processes: 2 };
  const future = JSON.stringify({ ...known, future_limit: 8 });
  const invalid = '{not json';
  const html = renderToStaticMarkup(<HarnessOutput sessions={[{ command: 'recorded command', output: [JSON.stringify(known), future, invalid] }]} />);
  expect(html).toContain('jobs: running 1 | queued 1 | heavy 1');
  expect(html).toContain('reserved: cpu 1000m | memory 67108864 B');
  expect(html).toContain('reserved: output 4096 B | processes 2');
  expect(html).toContain(future.replaceAll('"', '&quot;'));
  expect(html).toContain(invalid);
});
