import { cx } from './cx';
import { harnessFrame } from './frames/harness';
import type { HarnessFrame } from './frames';
import { HarnessOutput, harnessTerminalClass } from './harness-parts';
import type { ViewProps } from './CliView';

export interface HarnessViewProps extends ViewProps {
  frame?: HarnessFrame;
  /** Retained for callers; terminal output always uses one column. */
  layout?: 'stack' | 'split' | 'auto';
}

export function HarnessScreen({ frame }: { frame: HarnessFrame; layout?: 'stack' | 'split' | 'auto' }) {
  return (
    <>
      <HarnessOutput sessions={[frame]} />
      <p className="m-0">$ </p>
    </>
  );
}

/** Formatted terminal excerpt from the retained offline SDK recording. Page copy discloses its source and limits. */
export function HarnessView({ frame = harnessFrame, label, className }: HarnessViewProps) {
  return (
    <div data-band="paper" role={label ? 'group' : undefined} aria-label={label} className={cx(harnessTerminalClass, className)}>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden"><HarnessScreen frame={frame} /></div>
    </div>
  );
}
