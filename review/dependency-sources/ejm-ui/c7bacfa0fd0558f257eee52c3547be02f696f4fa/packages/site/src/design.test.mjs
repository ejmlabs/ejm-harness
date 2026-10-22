// @vitest-environment node
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative, sep } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { build, createServer } from 'vite';
import { ejmSite } from './vite.mjs';
import { siteDesignGuard } from './design.mjs';
import { ejmSsg } from '../../ssg/src/vite.mjs';

const fixtures = [];
const main = 'export default function Page() { return <h1>Shared site</h1>; }';
async function fixture(source = main, css = '@import "@ejm/ui/styles-site.css";') {
  const root = await mkdtemp(join(tmpdir(), 'ejm-design-'));
  fixtures.push(root);
  await mkdir(join(root, 'src'));
  await writeFile(join(root, 'src/main.tsx'), source);
  await writeFile(join(root, 'src/styles.css'), css);
  await writeFile(join(root, 'index.html'), '<html><head><!--ejm-head--></head><body><script type="module" src="/src/main.tsx"></script></body></html>');
  return root;
}
const chunks = result => (Array.isArray(result) ? result.flatMap(item => item.output) : result.output).filter(item => item.type === 'chunk');
const options = root => ({
  root, configFile: false, logLevel: 'silent', plugins: [ejmSite({ project: 'ejmlabs' })],
  build: { write: false, minify: false, lib: { entry: join(root, 'src/main.tsx'), formats: ['es'] }, rolldownOptions: { external: [/^@ejm\//, /^react\//] } },
});
afterEach(async () => { await Promise.all(fixtures.splice(0).map(root => rm(root, { recursive: true, force: true }))); });

describe('site design ownership in real Vite builds', () => {
  it.each([['default', {}], ['legacy false', { enforceDesignKit: false }]])('rejects visual overrides with %s options', async (_name, legacy) => {
    const root = await fixture('export default () => <h1 className="local">Hello</h1>;', '.local { color:red }');
    const config = options(root);
    config.plugins = [ejmSite({ project: 'ejmlabs', ...legacy })];
    await expect(build(config)).rejects.toThrow(/Consumer (?:className|CSS)/);
  });

  it('accepts public styles, static aliases, namespace data helpers and literal examples', async () => {
    const root = await fixture(`
      import { Static as Content } from '@ejm/ssg';
      import * as Docs from '@ejm/ui/docs';
      import { DocsPage } from '@ejm/ui/docs';
      const example = '<div className="local" style={{color:"red"}} />';
      // <style>.local { color: red }</style>
      export default function Page() { return <Content className="contents"><p>{example}</p><DocsPage {...Docs.docNavigation([], '/')} /></Content>; }
    `, '@import "@ejm/ui/styles-site.css" source(none);\n@import \'@ejm/demos/ide.css\';\n/* prose is not a class source */\n@source not "../content";');
    await expect(build(options(root))).resolves.toBeDefined();
  });

  it.each([
    ['local rule', '.local { color: red; }'],
    ['local import', '@import "./local.css";'],
    ['remote import', '@import "https://example.com/theme.css";'],
    ['private import', '@import "@ejm/ui/src/styles.css";'],
    ['theme', '@theme { --color-local: red; }'],
    ['utility', '@utility local { color: red; }'],
    ['animation', '@keyframes local { from { opacity: 0; } to { opacity: 1; } }'],
    ['apply', '@apply text-title;'],
    ['content injection', '@source inline("text-title");'],
    ['import scan override', '@import "@ejm/ui/styles-site.css" source("../");'],
  ])('rejects %s in an unused authored stylesheet', async (_name, css) => {
    const root = await fixture(main, css);
    await expect(build(options(root))).rejects.toThrow(/src\/styles\.css:1:1.*Consumer CSS/);
  });

  it.each([
    ['native classes', 'export default () => <h1 className="text-title">Hello</h1>;'],
    ['component classes', 'import { Hero } from "@ejm/ui"; export default () => <Hero className="local" title="Hello" />;'],
    ['inline style', 'export default () => <div style={{ color: "red" }} />;'],
    ['raw markup', 'export default () => <div dangerouslySetInnerHTML={{__html: "<style>h1 { color:red }</style>"}} />;'],
    ['stylesheet link', 'export default () => <link rel="stylesheet" href="/local.css" />;'],
    ['computed stylesheet relation', 'export default () => <link rel={"stylesheet"} href="/local.css" />;'],
    ['style element', 'export default () => <style>{"h1 { color:red }"}</style>;'],
    ['opaque props', 'const props = {}; export default () => <div {...props} />;'],
    ['untrusted static wrapper', 'const Static = () => null; export default () => <Static className="contents" />;'],
    ['computed static wrapper', 'import { Static } from "@ejm/ssg"; export default () => <Static className={"contents"} />;'],
    ['aliased hero override', 'import { Hero as Opening } from "@ejm/ui/layout/Hero"; export default () => <Opening title="Hello" motion="none" />;'],
    ['namespace heading override', 'import * as UI from "@ejm/ui"; export default () => <UI.SectionHead title="Hello" size="title" />;'],
    ['footer object override', 'import { SiteLayout } from "@ejm/ui"; const footer = { brand: "Local" }; export default () => <SiteLayout footer={{ ...footer }} />;'],
    ['dynamic private stylesheet import', 'void import("../custom.css"); export default () => <p>Hello</p>;'],
    ['private stylesheet import', 'import "../custom.css"; export default () => <p>Hello</p>;'],
  ])('rejects %s with an actionable source location', async (_name, source) => {
    const root = await fixture(source);
    await expect(build(options(root))).rejects.toThrow(/src\/main\.tsx:1:\d+/);
    await expect(build(options(root))).rejects.toThrow(/Extend the public ejm-ui contract/);
  });

  it('retains namespace Static and named shared data helper aliases', async () => {
    const root = await fixture(`import * as Ssg from '@ejm/ssg'; import { docNavigation as navigation, DocsPage } from '@ejm/ui/docs'; export default () => <Ssg.Static className="contents"><DocsPage {...navigation([], '/')} /></Ssg.Static>;`);
    await expect(build(options(root))).resolves.toBeDefined();
  });

  it('takes a shared recipe update without a consumer edit', async () => {
    const source = 'import { Hero } from "@ejm/ui/layout/Hero"; export default () => <Hero title="Hello" />;';
    const root = await fixture(source);
    const shared = join(root, 'shared-Hero.tsx');
    const config = options(root);
    config.resolve = { alias: { '@ejm/ui/layout/Hero': shared } };
    config.build.rolldownOptions.external = [/^react\//];
    await writeFile(shared, 'export function Hero({title}) { return <h1 className="shared-first">{title}</h1>; }');
    const first = await build(config);
    expect(chunks(first).map(item => item.code).join('\n')).toContain('shared-first');
    await writeFile(shared, 'export function Hero({title}) { return <h1 className="shared-second">{title}</h1>; }');
    const second = await build(config);
    expect(chunks(second).map(item => item.code).join('\n')).toContain('shared-second');
    expect(await readFile(join(root, 'src/main.tsx'), 'utf8')).toBe(source);
  });

  it('checks document requests and recovers when watcher events are missed', async () => {
    const root = await fixture();
    const server = await createServer({ ...options(root), server: { host: '127.0.0.1', port: 0, watch: null }, optimizeDeps: { noDiscovery: true } });
    try {
      await server.listen();
      const origin = server.resolvedUrls.local[0];
      const document = () => fetch(new URL('guide/', origin), { headers: { Accept: 'text/html' } });
      expect((await document()).status).toBe(200);
      const unused = join(root, 'src/unused.css');
      await writeFile(unused, '.local { color:red }');
      const response = await document();
      expect(response.status).toBe(500);
      expect(await response.text()).toContain('src/unused.css:1:1');
      expect((await fetch(new URL('theme-boot.js', origin))).status).toBe(500);
      await writeFile(unused, '@import "@ejm/ui/styles-site.css";');
      expect((await fetch(origin, { method: 'HEAD', headers: { Accept: 'text/html' } })).status).toBe(200);
      expect((await document()).status).toBe(200);
      expect((await fetch(new URL('theme-boot.js', origin))).status).toBe(200);
    } finally { await server.close(); }
  });

  it('blocks dev requests for newly added unused CSS and recovers when corrected', async () => {
    const root = await realpath(await fixture());
    let watcherReady = false;
    const events = [];
    const errors = [];
    const config = options(root);
    config.plugins.unshift({
      name: 'fixture-watch-ready',
      enforce: 'pre',
      configureServer(server) {
        server.watcher.once('ready', () => { watcherReady = true; });
        server.watcher.on('all', (event, file) => { events.push({ event, file: relative(root, file).split(sep).join('/') }); });
        server.watcher.on('error', error => { errors.push(String(error)); });
      },
    });
    const server = await createServer({ ...config, envDir: false, server: { host: '127.0.0.1', port: 0 }, optimizeDeps: { noDiscovery: true } });
    try {
      await server.listen();
      // A ready event can precede the native directory watch. Confirm a real source change first.
      let probe = 0;
      await expect.poll(async () => {
        const active = events.some(({ event, file }) => event === 'change' && file === 'src/styles.css');
        if (watcherReady && !active) await writeFile(join(root, 'src/styles.css'), `@import "@ejm/ui/styles-site.css"; /* watch probe ${++probe} */`);
        return { ready: watcherReady, active, errors: [...errors] };
      }, { timeout: 5000 }).toEqual({ ready: true, active: true, errors: [] });
      const origin = server.resolvedUrls.local[0];
      expect((await fetch(origin)).status).toBe(200);
      const unused = join(root, 'src/unused.css');
      const snapshot = async () => ({ status: (await fetch(origin)).status, events: [...events], errors: [...errors] });
      await writeFile(unused, '.local { color:red }');
      await expect.poll(snapshot, { timeout: 3000 }).toEqual({
        status: 500, events: expect.arrayContaining([{ event: 'add', file: 'src/unused.css' }]), errors: [],
      });
      expect(await (await fetch(origin)).text()).toContain('src/unused.css:1:1');
      await writeFile(unused, '@import "@ejm/ui/styles-site.css";');
      await expect.poll(snapshot, { timeout: 3000 }).toEqual({
        status: 200, events: expect.arrayContaining([{ event: 'change', file: 'src/unused.css' }]), errors: [],
      });
    } finally { await server.close(); }
  }, 15_000);

  it('accepts resolved footer data composition without allowing an opaque spread', async () => {
    const root = await fixture('import {SiteLayout} from "@ejm/ui"; const footer = {identity:{name:"Shared"}}; export default () => <SiteLayout footer={{...footer, columns:[]}} />;');
    await expect(build(options(root))).resolves.toBeDefined();
    await writeFile(join(root, 'src/main.tsx'), 'import {SiteLayout} from "@ejm/ui"; export default (footer) => <SiteLayout footer={{...footer}} />;');
    await expect(build(options(root))).rejects.toThrow(/cannot spread opaque props/);
  });

  it('preserves a design failure when prerendering is installed', async () => {
    const root = await fixture(main, '.local { color:red }');
    const config = options(root);
    config.plugins.push(ejmSsg({ site: 'src/site.tsx' }));
    await expect(build(config)).rejects.toThrow(/src\/styles\.css:1:1.*Consumer CSS/);
  });

  it('keeps only the owned gallery outside the consumer boundary', async () => {
    const root = await fixture('export default () => <div className="local" />;');
    // A consumer cannot escape by copying the gallery package name.
    await writeFile(join(root, 'package.json'), '{"name":"@ejm/gallery"}');
    expect(() => siteDesignGuard(root).validate()).toThrow(/Consumer className/);
  });
});
