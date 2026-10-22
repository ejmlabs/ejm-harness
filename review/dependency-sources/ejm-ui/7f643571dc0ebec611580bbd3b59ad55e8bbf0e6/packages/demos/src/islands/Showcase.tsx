import type { ReactNode, Ref } from 'react';
import { buttonBaseClass, buttonGhostClass, buttonMediumClass } from '@ejm/ui/control-recipes';
type Playback = 'idle' | 'running' | 'completed';

const playbackClass = `${buttonBaseClass} ${buttonGhostClass} ${buttonMediumClass}`;

export interface ShowcaseProps {
  name: string;
  children: ReactNode;
  playback?: Playback;
  paused?: boolean;
  host?: Ref<HTMLDivElement>;
  onPause?: () => void;
  onRestart?: () => void;
}

/** Product content remains readable to assistive technology; only playback controls are active. */
export function Showcase({ name, children, playback = 'idle', paused = false, host, onPause, onRestart }: ShowcaseProps) {
  const action = paused ? 'Resume' : 'Pause';
  return <div ref={host} data-showcase={name} data-playback={playback} role="group" aria-label={`${name} playback`} className="ejm-showcase @container min-w-0">
    {name === 'ejm-ide' && <p className="ejm-ide-scroll-hint text-sm text-fg">Scroll sideways to see the editor and assistant.</p>}
    <div data-showcase-viewport className="pointer-events-none relative aspect-[4/5] min-w-0 overflow-hidden sm:aspect-[16/10]">
      {children}
    </div>
    <div className="flex min-h-14 items-center justify-end gap-2 pt-2 text-sm text-fg">
      <button className={playbackClass} type="button" aria-label={`${action} ${name} playback`} onClick={onPause} disabled={!onPause || playback !== 'running'}>{action}</button>
      <button className={playbackClass} type="button" aria-label={`Restart ${name} playback`} onClick={onRestart} disabled={!onRestart}>Restart</button>
    </div>
  </div>;
}
