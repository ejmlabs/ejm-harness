import { useLayoutEffect, useRef } from 'react';
import { HarnessScreen } from '../previews/HarnessView';
import { harnessTerminalClass } from '../previews/harness-parts';
import { source, recording, create } from '../previews/harness';
import { validatePreviewRecording } from '../previews/recording';
import type { HarnessFrame } from '../previews/frames';
import { LoadedRecording } from './RecordedIsland';
validatePreviewRecording(source.id, recording);
function Frame({ frame }: { frame: HarnessFrame }) {
  const terminal = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => { const el = terminal.current; if (el) el.scrollTop = el.scrollHeight; }, [frame.output]);
  return <div data-band="paper" className={harnessTerminalClass}><div ref={terminal} className="min-h-0 flex-1 overflow-hidden"><HarnessScreen frame={frame} /></div></div>;
}
const renderFrame = (frame: HarnessFrame) => <Frame frame={frame} />;
export default function HarnessPreview() { return <LoadedRecording name="ejm-harness" source={source} recording={recording} create={create} renderFrame={renderFrame} />; }
