import { useTheme } from '@ejm/ui/hooks/useTheme';
import { IdeScreen } from '../previews/IdeView';
import { source, recording, create, type Surface } from '../previews/ide';
import { validatePreviewRecording } from '../previews/recording';
import { LoadedRecording } from './RecordedIsland';
validatePreviewRecording(source.id, recording);
function Frame({ surface }: { surface: Surface }) {
  const theme = useTheme().resolved;
  return <div data-theme={theme} className="h-full min-h-0"><IdeScreen frame={surface.frame} running={surface.running} folders={surface.folders} flash={surface.flash ?? undefined} passive /></div>;
}
const renderFrame = (surface: Surface) => <Frame surface={surface} />;
export default function IdePreview() { return <LoadedRecording name="ejm-ide" source={source} recording={recording} create={create} renderFrame={renderFrame} />; }
