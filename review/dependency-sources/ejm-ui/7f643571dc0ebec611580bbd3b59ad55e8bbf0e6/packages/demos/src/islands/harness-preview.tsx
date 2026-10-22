import { LazyPreview, type LazyPreviewOptions } from './LazyPreview';
const Server = import.meta.env.SSR ? (await import('../previews/harness-fallback')).default : null;
const load = () => import('./harness-preview-player');
/** A static native product excerpt, enhanced by a finite offline playback. */
export function LazyHarnessPreview(options: LazyPreviewOptions) { return <LazyPreview server={Server} load={load} {...options} />; }
