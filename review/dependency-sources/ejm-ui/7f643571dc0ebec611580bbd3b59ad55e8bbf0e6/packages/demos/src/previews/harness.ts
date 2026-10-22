import type { DemoOptions } from '../core';
import { createRecordedPlayback, type Recording, type RecordingSource } from '../recordings/playback';
import data from './recordings/harness.json';
import type { HarnessFrame } from './frames';
import { harnessFrame } from './frames/harness';

interface Update { line?: string; final?: HarnessFrame }
let at = 2000;
const beats = data.output.map((line, index) => {
  const beat = { at, update: { line } as Update };
  at += data.waits?.[index] ?? data.lineMs;
  return beat;
});
beats.push({ at, update: { final: harnessFrame } });
export const recording: Recording<Update> = { id: '9985083493447566a434b1f40ed22271889c06bf48e202c646d68fc813679433', beats };
export const source: RecordingSource<HarnessFrame, Update> = {
  id: recording.id,
  initialFrame: { command: data.command, output: [] },
  finalFrame: harnessFrame,
  load: async () => recording,
};
export function create(source: RecordingSource<HarnessFrame, Update>, recording: Recording<Update>, options: DemoOptions) {
  return createRecordedPlayback(source, recording, (frame, update) => update.final ?? { ...frame, output: [...frame.output, update.line!] }, options);
}
