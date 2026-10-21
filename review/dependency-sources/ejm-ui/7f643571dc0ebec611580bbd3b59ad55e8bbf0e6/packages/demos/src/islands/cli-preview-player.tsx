import { useLayoutEffect, useRef } from 'react';
import { CliScreen } from '../previews/CliView';
import { source, recording, create } from '../previews/cli';
import { validatePreviewRecording } from '../previews/recording';
import type { CliFrame } from '../previews/frames';
import { LoadedRecording } from './RecordedIsland';
validatePreviewRecording(source.id, recording);
function Frame({ frame }: { frame: CliFrame }) {
  const transcript = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => { const el = transcript.current; if (el) el.scrollTop = el.scrollHeight; }, [frame.blocks]);
  return <div className="ejm-cli-surface h-full min-h-0 bg-ui-ground p-[1em] font-mono text-[11px] text-ui-text sm:text-[12px]"><CliScreen frame={frame} transcript={transcript} /></div>;
}
const renderFrame = (frame: CliFrame) => <Frame frame={frame} />;
export default function CliPreview() { return <LoadedRecording name="ejm-cli" source={source} recording={recording} create={create} renderFrame={renderFrame} />; }
