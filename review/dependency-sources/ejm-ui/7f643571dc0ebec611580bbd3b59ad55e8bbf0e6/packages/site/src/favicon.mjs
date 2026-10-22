import { createHash } from 'node:crypto';

export const FAVICONS = Object.freeze(['favicon.ico', 'favicon.svg', 'favicon-16.png', 'favicon-32.png', 'favicon-48.png', 'apple-touch-icon.png', 'android-chrome-192.png', 'android-chrome-512.png', 'site.webmanifest']);

/** One build-only revision covers the icon family, including icons named by the manifest. */
export function versionedFavicons(sources) {
  const hash = createHash('sha256');
  for (const name of FAVICONS) hash.update(name).update('\0').update(sources[name]).update('\0');
  const version = hash.digest('hex').slice(0, 12);
  const manifest = JSON.parse(sources['site.webmanifest'].toString());
  for (const icon of manifest.icons ?? []) {
    const url = new URL(icon.src, 'https://ejm.invalid/');
    if (url.origin !== 'https://ejm.invalid' || !Object.hasOwn(sources, url.pathname.slice(1))) continue;
    url.searchParams.set('v', version);
    icon.src = (icon.src.startsWith('/') ? url.pathname : url.pathname.slice(1)) + url.search + url.hash;
  }
  return { version, sources: { ...sources, 'site.webmanifest': Buffer.from(JSON.stringify(manifest, null, 2) + '\n') } };
}
