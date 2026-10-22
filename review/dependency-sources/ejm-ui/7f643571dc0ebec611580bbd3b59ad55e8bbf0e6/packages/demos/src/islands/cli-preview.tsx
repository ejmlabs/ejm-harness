import { LazyPreview, type LazyPreviewOptions } from './LazyPreview';
const Server = import.meta.env.SSR ? (await import('../previews/cli-fallback')).default : null;
const load = () => import('./cli-preview-player');
/** A static native product excerpt, enhanced by a finite offline playback. */
export function LazyCliPreview(options: LazyPreviewOptions) { return <LazyPreview server={Server} load={load} {...options} />; }
