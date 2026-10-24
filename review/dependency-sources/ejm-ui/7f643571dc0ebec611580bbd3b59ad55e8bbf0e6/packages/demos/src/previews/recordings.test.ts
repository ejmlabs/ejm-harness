// @vitest-environment node
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { createManualClock } from '../core';
import type { Demo, DemoOptions } from '../core';
import type { PlaybackAction, PlaybackState, Recording, RecordingSource } from '../recordings/playback';
import * as cli from './cli';
import * as ide from './ide';
import * as harness from './harness';
import { validatePreviewRecording } from './recording';
import cliData from './recordings/cli.json';
import harnessData from './recordings/harness.json';
import cliReceipt from '../../fixtures/cli-output.json';
import harnessReceipt from '../../fixtures/harness-output.json';

async function check<Frame, Update>(source: RecordingSource<Frame, Update>, create: (source: RecordingSource<Frame, Update>, recording: Recording<Update>, options: DemoOptions) => Demo<PlaybackState<Frame>, PlaybackAction>) {
  const recording = await source.load();
  validatePreviewRecording(source.id, recording);
  const before = JSON.stringify({ source, recording });
  const clock = createManualClock();
  const player = create(source, recording, { clock });
  expect(player.state.frame).toEqual(source.finalFrame);
  player.dispatch({ type: 'start' });
  clock.runAll();
  expect(player.state.playback).toBe('completed');
  expect(player.state.frame).toEqual(source.finalFrame);
  expect(clock.pending).toBe(0);
  expect(JSON.stringify({ source, recording })).toBe(before);
  player.destroy();
}

describe('bundled native previews', () => {
  it('keeps retained capture and excerpt hashes aligned with the provenance record', () => {
    const provenance = readFileSync(new URL('./PROVENANCE.md', import.meta.url), 'utf8');
    const entries = [...provenance.matchAll(/\| `([^`]+)` \| `([a-f0-9]{64})` \|/g)];
    expect(entries).toHaveLength(8);
    for (const [, file, hash] of entries) {
      const bytes = readFileSync(new URL(`../../${file}`, import.meta.url));
      expect(createHash('sha256').update(bytes).digest('hex'), file).toBe(hash);
    }
  });
  it('retains observed stdout in the workflow rather than assigning authored results to commands', () => {
    expect(harnessData.output).toEqual(harnessReceipt.output);
    for (const record of cliReceipt.records) {
      const block = cliData.beats.flatMap(beat => beat.blocks ?? []).find(block => block.title === `done · shell ${record.command}`)!;
      expect(block.lines[0]!.t).toBe(`exit: Some(${record.exit}) | ${record.durationMs} ms`);
      expect(block.lines.slice(1).map(line => line.t).join('\n').trim()).toBe(record.stdout.trim());
    }
  });
  for (const [name, adapter] of Object.entries({ cli, ide, harness })) {
    it(`${name} identifies bounded data and retains its immutable final excerpt after playback`, async () => {
      const raw = readFileSync(new URL(`./recordings/${name}.json`, import.meta.url));
      expect(createHash('sha256').update(raw).digest('hex')).toBe(adapter.source.id);
      if (name === 'cli') await check(cli.source, cli.create);
      else if (name === 'ide') await check(ide.source, ide.create);
      else await check(harness.source, harness.create);
    });
  }
});
