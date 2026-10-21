import type { CliBlock } from '../cli';
import type { DemoOptions } from '../core';
import { createRecordedPlayback, type Recording, type RecordingSource } from '../recordings/playback';
import data from './recordings/cli.json';
import type { CliFrame } from './frames';
import { cliFrame } from './frames/cli';

interface Update { status?: Partial<CliFrame>; blocks?: readonly CliBlock[]; final?: CliFrame }
const beats = data.beats.map(({ at, ...update }) => ({ at, update: update as Update }));
beats.push({ at: beats.at(-1)!.at + 1500, update: { final: cliFrame } });
export const recording: Recording<Update> = { id: '35364d0b1b9dc10bed75d3e1925ef2a8a9cfebbde1d5f4924e893fe12bd3e0d8', beats };
export const source: RecordingSource<CliFrame, Update> = {
  id: recording.id,
  initialFrame: { ...cliFrame, blocks: [] },
  finalFrame: cliFrame,
  load: async () => recording,
};
export function create(source: RecordingSource<CliFrame, Update>, recording: Recording<Update>, options: DemoOptions) {
  return createRecordedPlayback(source, recording, (frame, update) => {
    if (update.final) return update.final;
    const blocks = new Map(frame.blocks.map(block => [block.id, block]));
    for (const block of update.blocks ?? []) blocks.set(block.id, block);
    return { ...frame, ...update.status, blocks: [...blocks.values()] };
  }, options);
}
