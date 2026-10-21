import type { Route, Site } from './site';

export interface HeadTag {
  tag: 'meta' | 'link';
  attrs: Record<string, string>;
}
export interface ResolvedHead {
  title: string;
  tags: HeadTag[];
}

/** Site head merged under the route head, with the title template, canonical and Open Graph defaults filled in. */
export function resolveHead(site: Site, route: Route): ResolvedHead {
  const s = site.head;
  const r = route.head;
  const own = r.title ?? s.title ?? '';
  const title = r.title && s.titleTemplate ? s.titleTemplate.replace('%s', r.title) : own;
  const description = r.description ?? s.description;
  const url = (u: string) => (u.startsWith('/') && site.origin ? site.origin + u : u);
  const canonical =
    r.canonical ?? s.canonical ?? (site.origin && route !== site.notFound ? site.origin + site.base + route.href : undefined);
  const og = { ...s.og, ...r.og };
  const tags: HeadTag[] = [];
  const meta = (k: 'name' | 'property', key: string, content?: string) => {
    if (content) tags.push({ tag: 'meta', attrs: { [k]: key, content } });
  };
  meta('name', 'description', description);
  meta('name', 'robots', r.robots ?? s.robots);
  if (canonical) tags.push({ tag: 'link', attrs: { rel: 'canonical', href: url(canonical) } });
  meta('property', 'og:title', og.title ?? title);
  meta('property', 'og:description', og.description ?? description);
  meta('property', 'og:type', og.type ?? 'website');
  meta('property', 'og:url', og.url ?? (canonical && url(canonical)));
  meta('property', 'og:image', og.image && url(og.image));
  meta('property', 'og:site_name', og.siteName);
  for (const m of [...(s.meta ?? []), ...(r.meta ?? [])]) {
    if (m.name) meta('name', m.name, m.content);
    else if (m.property) meta('property', m.property, m.content);
  }
  // Links from the site and the route are both kept; the same link twice is written once.
  const seen = new Set<string>();
  for (const link of [...(s.links ?? []), ...(r.links ?? [])]) {
    const attrs = Object.fromEntries(Object.entries(link).filter((e): e is [string, string] => e[1] !== undefined));
    const key = JSON.stringify(Object.entries(attrs).sort(([a], [b]) => a.localeCompare(b)));
    if (Object.keys(attrs).length && !seen.has(key)) (seen.add(key), tags.push({ tag: 'link', attrs }));
  }
  return { title, tags };
}

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** Marked `data-ejm` so client navigation can swap them. */
export const tagToString = (t: HeadTag) =>
  `<${t.tag} ${Object.entries(t.attrs).map(([k, v]) => `${k}="${esc(v)}"`).join(' ')} data-ejm>`;

export const headToString = (h: ResolvedHead) => `<title>${esc(h.title)}</title>\n${h.tags.map(tagToString).join('\n')}`;

let headRevision = 0;

export function applyHead(h: ResolvedHead) {
  const revision = ++headRevision;
  document.title = h.title;
  const previous = [...document.head.querySelectorAll('[data-ejm]')];
  const oldStyles = previous.filter(n => n.matches('link[rel="stylesheet"]'));
  previous.filter(n => !oldStyles.includes(n)).forEach(n => n.remove());
  const styleLoads: Promise<void>[] = [];
  const nextStyles = new Set<Element>();
  for (const t of h.tags) {
    // Reuse a sheet already painted by this document, including rapid back navigation.
    const existing = t.tag === 'link' && t.attrs.rel === 'stylesheet'
      ? oldStyles.find(n => (n as HTMLLinkElement).sheet
        && Object.entries(t.attrs).every(([name, value]) => n.getAttribute(name) === value)
        && n.getAttributeNames().every(name => name === 'data-ejm' || name in t.attrs))
      : undefined;
    // A failed or still-pending link has no sheet. Recreate it on a new request
    // so recovery cannot wait forever for a load event that already failed.
    if (!existing && t.tag === 'link' && t.attrs.rel === 'stylesheet') {
      oldStyles.filter(n => n.getAttribute('href') === t.attrs.href && !(n as HTMLLinkElement).sheet).forEach(n => n.remove());
    }
    const el = existing ?? document.createElement(t.tag);
    for (const [k, v] of Object.entries(t.attrs)) el.setAttribute(k, v);
    el.setAttribute('data-ejm', '');
    if (t.tag === 'link' && t.attrs.rel === 'stylesheet') {
      nextStyles.add(el);
      if (!(el as HTMLLinkElement).sheet) styleLoads.push(new Promise(resolve => {
        el.addEventListener('load', () => resolve(), { once: true });
        // Keep the current sheet if its replacement cannot load.
      }));
    }
    // Moving a reused link also preserves the destination's stylesheet cascade.
    document.head.append(el);
  }
  // Keep the current paint until every replacement sheet is ready. A newer
  // navigation owns its own set, so an older completion cannot remove its CSS.
  const current = [...nextStyles];
  void Promise.all(styleLoads).then(() => {
    if (revision !== headRevision || current.some(el => !el.isConnected)) return;
    oldStyles.filter(el => !nextStyles.has(el)).forEach(el => el.remove());
  });
}
