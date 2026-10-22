import { act, screen, waitFor } from '@testing-library/react';
import { hydrateRoot, type Root } from 'react-dom/client';
import { renderToString } from 'react-dom/server';
import { REDUCED_MOTION_QUERY } from '@ejm/ui/hooks/useReducedMotion';
import { afterEach, expect, it, vi } from 'vitest';
import { createManualClock, type DemoOptions } from '../core';
import '../test-helpers';
import { createRecordedPlayback, type Recording, type RecordingSource } from '../recordings/playback';
import { LoadedRecording } from './RecordedIsland';
import { LazyPreview } from './LazyPreview';

afterEach(() => vi.restoreAllMocks());

it('hydrates a bundled reduced-motion preview without loading twice or starting a player', async () => {
  let reduced = true;
  const listeners = new Set<() => void>();
  vi.spyOn(window, 'matchMedia').mockImplementation(query => ({
    get matches() { return query === REDUCED_MOTION_QUERY && reduced; },
    media: query, onchange: null,
    addEventListener: (_event: string, listener: EventListenerOrEventListenerObject) => { if (query === REDUCED_MOTION_QUERY) listeners.add(listener as () => void); },
    removeEventListener: (_event: string, listener: EventListenerOrEventListenerObject) => { if (query === REDUCED_MOTION_QUERY) listeners.delete(listener as () => void); },
    addListener() {}, removeListener() {}, dispatchEvent: () => false,
  }));
  const clock = createManualClock();
  const source: RecordingSource<string, string> = { id: 'a'.repeat(64), initialFrame: 'first test frame', finalFrame: 'final test frame', load: vi.fn() };
  const recording: Recording<string> = { id: source.id, beats: [{ at: 0, update: 'first test frame' }, { at: 100, update: 'final test frame' }] };
  const create = vi.fn((frames: RecordingSource<string, string>, data: Recording<string>, options: DemoOptions) => createRecordedPlayback(frames, data, (_frame, update) => update, options));
  const renderFrame = (frame: string) => <p>{frame}</p>;
  const Final = () => renderFrame(source.finalFrame);
  const Player = () => <LoadedRecording name="test fixture" source={source} recording={recording} create={create} renderFrame={renderFrame} options={{ clock }} />;
  const tree = <LazyPreview eager server={Final} load={async () => ({ default: Player })} />;
  const container = document.createElement('div');
  container.innerHTML = renderToString(tree);
  document.body.append(container);
  let root: Root | undefined;
  try {
    await act(async () => { root = hydrateRoot(container, tree); });
    await waitFor(() => expect(container.querySelector('[data-showcase]')).not.toBeNull());
    expect(screen.getByText('final test frame')).toBeVisible();
    expect(create).not.toHaveBeenCalled();
    expect(source.load).not.toHaveBeenCalled();
    expect(clock.pending).toBe(0);
    await act(async () => { reduced = false; listeners.forEach(listener => listener()); });
    expect(screen.getByText('first test frame')).toBeVisible();
    expect(clock.pending).toBe(1);
    act(() => { reduced = true; listeners.forEach(listener => listener()); });
    expect(screen.getByText('final test frame')).toBeVisible();
    expect(clock.pending).toBe(0);
    await act(async () => { reduced = false; listeners.forEach(listener => listener()); });
    expect(screen.getByText('first test frame')).toBeVisible();
  } finally {
    act(() => root?.unmount());
    container.remove();
  }
  expect(clock.pending).toBe(0);
  expect(listeners.size).toBe(0);
  expect(source.load).not.toHaveBeenCalled();
});
