import type { DemoOptions } from '../core';
import type { ChatItem } from '../ide';
import { createRecordedPlayback, type Recording, type RecordingSource } from '../recordings/playback';
import data from './recordings/ide.json';
import type { IdeFrame } from './frames';
import { ideFrame } from './frames/ide';

type TextUpdate = [keep: number, suffix: string];
interface Update {
  final?: Surface;
  frame?: Partial<Omit<IdeFrame, 'chat' | 'composer'>>;
  composer?: TextUpdate;
  chat?: { id: number; set?: Partial<ChatItem>; text?: TextUpdate }[];
  running?: boolean;
  flash?: number[] | null;
  folders?: Record<string, boolean>;
}
export interface Surface { frame: IdeFrame; running: boolean; flash: number[] | null; folders: Record<string, boolean> }
// Group 20ms text chunks in pairs. End times and every character stay intact;
// this authored preview updates text at most 25 times per second.
interface Stream { at: number; step: number; stream: 'composer' | number; keep: number; text: string; sizes: number[] }
const input = data as unknown as { beats: ({ at: number } & Update | Stream)[] };
const beats = input.beats.flatMap(entry => {
  if (!('stream' in entry)) { const { at, ...update } = entry; return [{ at, update: update as Update }]; }
  let offset = 0;
  const result: { at: number; update: Update }[] = [];
  const group = Math.max(1, Math.ceil(40 / entry.step));
  for (let index = 0; index < entry.sizes.length; index += group) {
    const stop = Math.min(index + group, entry.sizes.length);
    const size = entry.sizes.slice(index, stop).reduce((sum, value) => sum + value, 0);
    const text: TextUpdate = [entry.keep + offset, entry.text.slice(offset, offset + size)];
    offset += size;
    result.push({ at: entry.at + (stop - 1) * entry.step, update: entry.stream === 'composer' ? { composer: text } : { chat: [{ id: entry.stream, text }] } });
  }
  return result;
});
export const recording: Recording<Update> = { id: '093840636c87fa0d98a1580ab55a87812de222d7f95eaa84ac3b7cbbc030917c', beats };
const finalFrame: Surface = { frame: ideFrame, running: false, flash: null, folders: { src: true, web: true } };
beats.push({ at: beats.at(-1)!.at + 1500, update: { final: finalFrame } });
export const source: RecordingSource<Surface, Update> = {
  id: recording.id,
  initialFrame: { ...finalFrame, frame: { ...ideFrame, chat: [], composer: '' } },
  finalFrame,
  load: async () => recording,
};
const text = (before: string, [keep, suffix]: TextUpdate) => before.slice(0, keep) + suffix;
export function create(source: RecordingSource<Surface, Update>, recording: Recording<Update>, options: DemoOptions) {
  return createRecordedPlayback(source, recording, (surface, update) => {
    if (update.final) return update.final;
    const frame = { ...surface.frame, ...update.frame, chat: [...surface.frame.chat] };
    if (update.composer) frame.composer = text(frame.composer ?? '', update.composer);
    for (const change of update.chat ?? []) {
      const index = frame.chat.findIndex(item => item.id === change.id);
      const item = { ...(index < 0 ? { id: change.id } : frame.chat[index]), ...change.set } as ChatItem;
      if (change.text) Object.assign(item, { text: text('text' in item ? item.text : '', change.text) });
      if (index < 0) frame.chat.push(item); else frame.chat[index] = item;
    }
    return { frame, running: update.running ?? surface.running, flash: update.flash === undefined ? surface.flash : update.flash, folders: update.folders ?? surface.folders };
  }, options);
}
