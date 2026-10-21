// @vitest-environment node
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { checkNativeDesign } from './native.mjs';

const fixtures = [];
async function fixture(source = '') {
  const root = await mkdtemp(join(tmpdir(), 'ejm-native-design-'));
  fixtures.push(root);
  await mkdir(join(root, 'src'));
  await writeFile(join(root, 'src/page.ts'), source);
  return root;
}
const check = (root, documents = []) => checkNativeDesign({ root, stylesheetHref: '/shared/ui.css', documents });
afterEach(async () => { await Promise.all(fixtures.splice(0).map(root => rm(root, { recursive: true, force: true }))); });

describe('native HTML design ownership', () => {
  it('accepts semantic native forms, shared recipes, text nodes and the stylesheet compatibility import', async () => {
    const root = await fixture(`const page = title => \`<form method="post"><div data-ejm="form-field"><label for="name">Name</label><input data-ejm="field" id="name" required></div><button data-ejm="button" type="submit">\${title}</button></form>\`; const node = document.createElement('p'); node.dataset.ejm = 'notice'; node.dataset.tone = 'success'; node.textContent = 'Saved';`);
    await writeFile(join(root, 'src/styles.css'), '@import url("/shared/ui.css");');
    expect(check(root, [{ name: '/privacy', html: '<link rel="stylesheet" href="/shared/ui.css"><main data-ejm="page"><h1>Choices</h1></main>' }]).documents).toBe(1);
  });

  it('accepts focused setup, native connection disclosure and a hidden session', async () => {
    const root = await fixture(`const region = document.getElementById('content'); region.dataset.ejm = 'setup'; const header = document.getElementById('connection'); header.dataset.ejm = 'workspace-header';`);
    const setup = '<main id="content" data-ejm="setup" tabindex="-1"><header data-ejm="heading"><h1>Connect</h1></header><form data-ejm="stack"><label for="invite">Invitation</label><input id="invite" data-ejm="field" required><button data-ejm="button" type="submit">Connect</button></form><section data-ejm="session" hidden></section></main>';
    const session = '<main id="content" data-ejm="workspace" tabindex="-1"><header id="connection" data-ejm="workspace-header"><h1>Session</h1><details><summary>Connection details</summary><dl data-ejm="metadata"><dt>Host</dt><dd data-ejm="fingerprint">Public identity</dd></dl></details></header><section data-ejm="session"><div data-ejm="transcript"></div><form data-ejm="composer"><textarea data-ejm="field" aria-label="Message"></textarea><button data-ejm="button" type="submit">Send</button></form></section></main>';
    expect(check(root, [{ name: '/connect', html: setup }, { name: '/session', html: session }]).documents).toBe(2);
  });

  it('accepts related catalog content, native checkbox labels and hidden form values', async () => {
    const root = await fixture('const item = document.createElement("article"); item.dataset.ejm = "group";');
    const html = '<ul data-ejm="list"><li><article data-ejm="group"><h2>harbor-search</h2><p>Local notes.</p><p data-ejm="muted">Version 1.2.0</p><pre>ejm packages install harbor-search</pre></article></li></ul><form><input type="hidden" name="token" value="fixture"><label data-ejm="check"><input type="checkbox" required><span>Agree</span></label><div hidden><input data-ejm="field"><select data-ejm="field"><option>One</option></select><textarea data-ejm="field"></textarea></div><button data-ejm="button" type="reset">Reset</button></form>';
    expect(check(root, [{ name: '/catalog', html }]).documents).toBe(1);
  });

  it.each([
    ['unmarked button', '<button type="button">Continue</button>'],
    ['implicit submit', '<button data-ejm="button">Continue</button>'],
    ['unmarked input', '<input name="search">'],
    ['wrong field recipe', '<input data-ejm="group">'],
    ['unmarked select', '<select><option>One</option></select>'],
    ['unmarked textarea', '<textarea></textarea>'],
    ['unowned checkbox', '<label><input type="checkbox">Agree</label>'],
    ['unowned hidden-region control', '<div hidden><input></div>'],
  ])('rejects %s in templates and finished documents', async (_name, html) => {
    const root = await fixture(`export const page = ${JSON.stringify(html)};`);
    expect(() => check(root)).toThrow(/src\/page\.ts:1:\d+.*Native/);
    const empty = await fixture();
    expect(() => check(empty, [{ name: '/catalog', html }])).toThrow(/\/catalog:1:\d+.*Native/);
  });

  it.each([
    ['template class', 'export const page = `<h1 class="local">Title</h1>`;'],
    ['template inline style', 'export const page = `<input style="width:20rem">`;'],
    ['HTML visual attribute', 'export const page = `<h1 align="center">Title</h1>`;'],
    ['local drawing', 'export const page = `<svg><path d="M0 0" /></svg>`;'],
    ['unknown recipe', 'export const page = `<section data-ejm="local-card"></section>`;'],
    ['unknown state', 'export const page = `<p data-ejm="notice" data-tone="purple"></p>`;'],
    ['injected style', 'export const page = `<template><style>body{color:red}</style></template>`;'],
    ['local stylesheet', 'export const page = `<link rel="stylesheet" href="/local.css">`;'],
    ['private CSS import', 'import "../local.css";'],
    ['dynamic CSS import', 'void import("../local.css");'],
    ['DOM classes', 'document.createElement("p").classList.add("local");'],
    ['DOM styles', 'const node = document.createElement("p"); node.style.color = "red";'],
    ['DOM attribute', 'node.setAttribute("class", "local");'],
    ['DOM raw markup', 'node.innerHTML = "<p>Local</p>";'],
    ['DOM recipe', 'node.dataset.ejm = "local-card";'],
    ['embedded account script', 'const script = String.raw`node.setAttribute("style", "color:red");`;'],
  ])('rejects %s at its authored location', async (_name, source) => {
    const root = await fixture(source);
    expect(() => check(root)).toThrow(/src\/page\.ts:1:\d+/);
  });

  it('checks unused authored CSS and the final composed documents', async () => {
    const root = await fixture();
    await writeFile(join(root, 'src/unused.css'), '.local { padding: 1rem }');
    expect(() => check(root)).toThrow(/src\/unused\.css:1:1.*Consumer CSS/);
    await writeFile(join(root, 'src/unused.css'), '@import "@ejm/ui/static.css";');
    await writeFile(join(root, 'src/local.scss'), '.local { color: red }');
    expect(() => check(root)).toThrow(/src\/local\.scss:1:1/);
    await rm(join(root, 'src/local.scss'));
    expect(() => check(root, [{ name: '/generated-account', html: '<main><h1 class="local">Account</h1></main>' }])).toThrow(/\/generated-account:1:\d+.*Consumer class/);
  });

  it('does not treat escaped user content or literal CSS explanations as markup', async () => {
    const root = await fixture('const example = "class=local; color:red"; const escaped = "&lt;h1 class=local&gt;";');
    expect(check(root, [{ name: '/escaped', html: '<pre>&lt;p class="local"&gt;</pre>' }]).files).toHaveLength(1);
  });
});
