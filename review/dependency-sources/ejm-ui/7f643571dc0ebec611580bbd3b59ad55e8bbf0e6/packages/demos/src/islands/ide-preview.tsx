import { LazyPreview, type LazyPreviewOptions } from './LazyPreview';
const Server = import.meta.env.SSR ? (await import('../previews/ide-fallback')).default : null;
const load = () => import('./ide-preview-player');
/** A static native product excerpt, enhanced by a finite offline playback. */
export function LazyIdePreview(options: LazyPreviewOptions) { return <LazyPreview server={Server} load={load} {...options} />; }
