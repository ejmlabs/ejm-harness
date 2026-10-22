import { afterEach, expect, it } from 'vitest';
import { applyHead } from './head';

const head = (href: string) => ({ title: href, tags: [{ tag: 'link' as const, attrs: { rel: 'stylesheet', href } }] });
const sheet = (href: string) => document.head.querySelector<HTMLLinkElement>(`link[href="${href}"]`)!;
const loaded = async (href: string) => { sheet(href).dispatchEvent(new Event('load')); await Promise.resolve(); await Promise.resolve(); };
afterEach(() => { document.head.innerHTML = ''; });

it('keeps the painted stylesheet until its replacement loads', async () => {
  applyHead(head('/first.css'));
  await loaded('/first.css');
  applyHead(head('/second.css'));
  expect(sheet('/first.css')).not.toBeNull();
  await loaded('/second.css');
  expect(sheet('/first.css')).toBeNull();
  expect(sheet('/second.css')).not.toBeNull();
});

it('lets only the latest navigation remove earlier stylesheets', async () => {
  applyHead(head('/first.css'));
  await loaded('/first.css');
  applyHead(head('/second.css'));
  applyHead(head('/third.css'));
  await loaded('/second.css');
  expect(sheet('/first.css')).not.toBeNull();
  expect(sheet('/third.css')).not.toBeNull();
  await loaded('/third.css');
  expect([...document.head.querySelectorAll('link')].map(el => el.getAttribute('href'))).toEqual(['/third.css']);
});


it('recreates a failed stylesheet on the next navigation attempt', async () => {
  applyHead(head('/painted.css'));
  await loaded('/painted.css');
  applyHead(head('/retry.css'));
  const failed = sheet('/retry.css');
  failed.dispatchEvent(new Event('error'));
  applyHead(head('/retry.css'));
  expect(sheet('/retry.css')).not.toBe(failed);
  expect(failed.isConnected).toBe(false);
  expect(sheet('/painted.css')).not.toBeNull();
  await loaded('/retry.css');
  expect(sheet('/painted.css')).toBeNull();
});

it('does not carry an old media constraint into a route that omits it', async () => {
  const first = head('/shared.css');
  Object.assign(first.tags[0]!.attrs, { media: 'print' });
  applyHead(first);
  const printed = sheet('/shared.css');
  Object.defineProperty(printed, 'sheet', { value: new CSSStyleSheet() });
  await loaded('/shared.css');
  applyHead(head('/shared.css'));
  const links = [...document.head.querySelectorAll('link')];
  const next = links.find(el => el !== printed)!;
  expect(next.hasAttribute('media')).toBe(false);
  next.dispatchEvent(new Event('load'));
  await Promise.resolve(); await Promise.resolve();
  expect(printed.isConnected).toBe(false);
  expect(next.isConnected).toBe(true);
});

it('keeps destination cascade order when adding a sheet before a reused one', async () => {
  applyHead(head('/override.css'));
  const override = sheet('/override.css');
  Object.defineProperty(override, 'sheet', { value: new CSSStyleSheet() });
  await loaded('/override.css');

  applyHead({ title: 'Destination', tags: [
    { tag: 'link', attrs: { rel: 'stylesheet', href: '/base.css' } },
    { tag: 'link', attrs: { rel: 'stylesheet', href: '/override.css' } },
  ] });
  expect([...document.head.querySelectorAll('link[rel="stylesheet"]')].map(el => el.getAttribute('href')))
    .toEqual(['/base.css', '/override.css']);
  await loaded('/base.css');
  expect(sheet('/override.css')).toBe(override);
  expect([...document.head.querySelectorAll('link[rel="stylesheet"]')].map(el => el.getAttribute('href')))
    .toEqual(['/base.css', '/override.css']);
});
