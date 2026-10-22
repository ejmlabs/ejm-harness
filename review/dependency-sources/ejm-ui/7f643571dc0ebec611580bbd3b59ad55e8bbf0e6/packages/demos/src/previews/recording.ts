import { validateRecording, type Recording } from '../recordings/playback';

/** Bundled data uses the published finite recording and size bounds. */
export function validatePreviewRecording<Update>(id: string, recording: Recording<Update>) {
  validateRecording(id, recording);
  if (JSON.stringify(recording).length > 262_144) throw new Error('Preview recording is too large');
}
