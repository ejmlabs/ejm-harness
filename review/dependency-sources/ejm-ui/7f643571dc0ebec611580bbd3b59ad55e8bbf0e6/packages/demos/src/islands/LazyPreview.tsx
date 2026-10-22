import { useEffect, useRef, useState, type ComponentType } from 'react';
import type { LazyIslandOptions } from './LazyIsland';
export type LazyPreviewOptions = LazyIslandOptions;
interface Props extends LazyPreviewOptions { server: ComponentType | null; load: () => Promise<{ default: ComponentType }> }

/** Preserve complete server HTML until the per-product player is close enough to load. */
export function LazyPreview({ server: Server, load, rootMargin = '240px', eager = false }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const [Loaded, setLoaded] = useState<ComponentType | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    const element = host.current;
    if (!element) return;
    let disposed = false;
    let loading = false;
    const go = () => {
      if (loading) return;
      loading = true;
      observer?.disconnect();
      void load().then(module => { if (!disposed) setLoaded(() => module.default); })
        .catch(() => { if (!disposed) setFailed(true); });
    };
    let observer: IntersectionObserver | undefined;
    if (eager || !element.hasChildNodes() || typeof IntersectionObserver === 'undefined') go();
    else {
      observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) go(); }, { rootMargin });
      observer.observe(element);
    }
    return () => { disposed = true; observer?.disconnect(); };
  }, [eager, load, rootMargin]);
  if (Loaded) return <div ref={host}><Loaded /></div>;
  return <>
    {Server ? <div ref={host}><Server /></div> : <div ref={host} suppressHydrationWarning dangerouslySetInnerHTML={{ __html: '' }} />}
    {failed && <p role="status">Playback is unavailable. The recorded excerpt remains visible. Reload to try again.</p>}
  </>;
}
