import { LazyCliPreview } from '@ejm/demos/islands/cli-preview';
import { LazyIdePreview } from '@ejm/demos/islands/ide-preview';
import { LazyHarnessPreview } from '@ejm/demos/islands/harness-preview';
import { Band, SectionHead } from '@ejm/ui';

// Local recorded workflows. Each section keeps a complete static outcome before its playback loads.

export default function Demos() {
  return (
    <>
      <Band tone="paper" aria-labelledby="demos-title">
        <SectionHead purpose="page" kicker="Demos" title="Watch the three products." id="demos-title" as="h1">
          <p className="m-0">Recorded product workflows. Pause, resume or restart each playback.</p>
        </SectionHead>
      </Band>
      <Band id="cli" tone="mist" aria-labelledby="cli-title">
        <SectionHead title="ejm-cli" id="cli-title">
          <p className="m-0">A retry fix, its patch and three recorded passing tests. This local playback runs no commands or model calls.</p>
        </SectionHead>
        <div className="mt-16"><LazyCliPreview /></div>
      </Band>
      <Band id="ide" tone="sage" aria-labelledby="ide-title">
        <SectionHead title="ejm-ide" id="ide-title">
          <p className="m-0">Two reviewed edits and five added boundary tests. The tests were not run; this local playback uses no model connection.</p>
        </SectionHead>
        <div className="mt-16"><LazyIdePreview /></div>
      </Band>
      <Band id="harness" tone="paper" aria-labelledby="harness-title">
        <SectionHead title="ejm-harness" id="harness-title">
          <p className="m-0">Formatted local recording. No worker or model runs here. Resource figures are admission reservations.</p>
        </SectionHead>
        <div className="mt-16"><LazyHarnessPreview /></div>
      </Band>
    </>
  );
}
