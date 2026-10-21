import { REDUCED_MOTION_QUERY, useReducedMotion } from '@ejm/ui/hooks/useReducedMotion';
import { useEffect, useState, type ReactNode } from 'react';
import type { Demo, DemoOptions } from '../core';
import type { PlaybackAction, PlaybackState, Recording, RecordingSource } from '../recordings/playback';
import { RecordedView } from '../views/RecordedView';
import { useDemo } from './useDemo';

export type RecordedOptions = Omit<DemoOptions, 'visible' | 'reducedMotion'>;
interface RecordedIslandProps<Frame, Update> {
  name: string;
  source: RecordingSource<Frame, Update>;
  options?: RecordedOptions;
  validate(id: string, recording: Recording<Update>): void;
  create(source: RecordingSource<Frame, Update>, recording: Recording<Update>, options: DemoOptions): Demo<PlaybackState<Frame>, PlaybackAction>;
  renderFrame(frame: Frame): ReactNode;
}

function Player<Frame, Update>({ name, source, recording, options, create, renderFrame }: Omit<RecordedIslandProps<Frame, Update>, 'validate'> & { recording: Recording<Update> }) {
  const { state, dispatch, host } = useDemo((opts) => create(source, recording, opts), { ...options, reducedMotion: false });
  useEffect(() => {
    const update = () => dispatch({ type: 'page-visible', value: document.visibilityState !== 'hidden' });
    update();
    document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  }, [dispatch]);
  useEffect(() => {
    if (state.playback === 'idle') dispatch({ type: 'start' });
  }, [dispatch, state.playback]);
  const restart = () => { dispatch({ type: 'reset' }); dispatch({ type: 'start' }); };
  return (
    <RecordedView name={name} playback={state.playback} paused={state.paused} host={host} onPause={() => dispatch({ type: state.paused ? 'resume' : 'pause' })} onRestart={restart}>
      {renderFrame(state.frame)}
    </RecordedView>
  );
}

/** Bundled previews arrive with validated data through their existing lazy module boundary. */
export function LoadedRecording<Frame, Update>(props: Omit<RecordedIslandProps<Frame, Update>, 'validate'> & { recording: Recording<Update> }) {
  const reduced = useReducedMotion();
  return reduced
    ? <RecordedView name={props.name}>{props.renderFrame(props.source.finalFrame)}</RecordedView>
    : <Player {...props} />;
}

/** Shared lazy data and lifecycle boundary. Product content and validation stay with the supplied adapter. */
export function RecordedIsland<Frame, Update>({ name, source, options, validate, create, renderFrame }: RecordedIslandProps<Frame, Update>) {
  const reduced = useReducedMotion();
  const [loaded, setLoaded] = useState<{ source: RecordingSource<Frame, Update>; recording: Recording<Update> }>();
  const [failedSource, setFailedSource] = useState<RecordingSource<Frame, Update>>();
  useEffect(() => {
    if (reduced || loaded?.source === source) return;
    let gone = false;
    setFailedSource(undefined);
    Promise.resolve().then(async () => {
      // Hydration first uses the server's false snapshot. Read the same query before importing data.
      if (gone || (typeof window.matchMedia === 'function' && window.matchMedia(REDUCED_MOTION_QUERY).matches)) return;
      const recording = await source.load();
      validate(source.id, recording);
      if (!gone) setLoaded({ source, recording });
    }).catch(() => { if (!gone) setFailedSource(source); });
    return () => { gone = true; };
  }, [source, reduced, loaded?.source, validate]);
  if (!reduced && loaded?.source === source) return <Player key={source.id} name={name} source={source} recording={loaded.recording} options={options} create={create} renderFrame={renderFrame} />;
  return (
    <>
      <RecordedView name={name}>{renderFrame(source.finalFrame)}</RecordedView>
      {failedSource === source && <p role="status">Playback is unavailable. The recorded final frame is shown.</p>}
    </>
  );
}
