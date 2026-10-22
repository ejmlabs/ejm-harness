// Node-only Markdown compiler. No browser entry imports this module.
import { decodeHTML } from 'entities';
import GithubSlugger, { slug } from 'github-slugger';
import { Lexer } from 'marked';
import { parseDocument } from 'yaml';

export { slug };
const CALLOUTS = { NOTE: 'note', TIP: 'tip', WARNING: 'warning', IMPORTANT: 'warning', CAUTION: 'warning' };
const safeHref = (value) => {
  const href = decodeHTML(value).trim();
  if (/[\u0000-\u0020\u007f]/.test(href) || (/^[a-z][a-z0-9+.-]*:/i.test(href) && !/^(https?:|mailto:|tel:)/i.test(href)))
    throw new Error(`markdown: unsafe link ${href}`);
  return href;
};
const defaultLink = (href) => ({ href, external: /^(?:https?:)?\/\//i.test(href) });
const plain = (tokens = []) => tokens.map((t) => t.type === 'codespan' ? t.text : t.tokens ? plain(t.tokens) : decodeHTML(t.text ?? '')).join('');

function frontMatter(source, required) {
  const match = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/.exec(source);
  if (!match) {
    if (required) throw new Error('markdown: missing front matter');
    return { meta: {}, body: source };
  }
  const doc = parseDocument(match[1], { schema: 'failsafe', uniqueKeys: true });
  if (doc.errors.length) throw new Error(`markdown: invalid front matter: ${doc.errors[0].message}`);
  const meta = doc.toJS({ maxAliasCount: 0 });
  if (!meta || typeof meta !== 'object' || Array.isArray(meta) || Object.values(meta).some((v) => typeof v !== 'string'))
    throw new Error('markdown: front matter must contain scalar text fields');
  return { meta, body: source.slice(match[0].length) };
}

function inline(tokens, link) {
  return tokens.flatMap((t) => {
    switch (t.type) {
      case 'text': case 'escape': return t.tokens ? inline(t.tokens, link) : [decodeHTML(t.text)];
      case 'codespan': return [['code', t.text]];
      case 'strong': case 'em': case 'del': return [[t.type, inline(t.tokens, link)]];
      case 'br': return [['br']];
      case 'link': {
        const result = link(safeHref(t.href));
        return [['a', safeHref(result.href), !!result.external, inline(t.tokens, link)]];
      }
      case 'html':
        if (/^<br\s*\/?>$/i.test(t.raw)) return [['br']];
        throw new Error(`markdown: inline HTML is not supported: ${t.raw}`);
      default: throw new Error(`markdown: inline ${t.type} is not supported`);
    }
  });
}

function codeBlock(token) {
  const info = (token.lang ?? '').trim();
  const lang = /^\S+/.exec(info)?.[0] ?? 'text';
  const attrs = Object.fromEntries([...info.matchAll(/(\w+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
  const remainder = info.slice(lang.length).trim();
  const label = attrs.label ?? attrs.title ?? (remainder && !/\w+="/.test(remainder) ? remainder : undefined);
  return { type: 'code', lang, code: token.text, ...(label ? { label } : {}), ...(attrs.tab ? { tab: attrs.tab } : {}) };
}

/** A marked token adapter, with all product route policy supplied by the caller. */
export function parseDoc(source, { link = defaultLink, metadata = {}, requireFrontMatter = false, requiredMetadata = [], requireExplicitIds = false } = {}) {
  const { meta, body } = frontMatter(source, requireFrontMatter);
  const fields = { ...meta, ...metadata };
  for (const key of requiredMetadata) if (fields[key] === undefined || fields[key] === '') throw new Error(`markdown: front matter needs ${key}`);
  if (fields.order !== undefined && !Number.isFinite(Number(fields.order))) throw new Error('markdown: front matter order must be a number');
  const lexer = new Lexer({ gfm: true });
  const tokens = lexer.lex(body);
  const slugs = new GithubSlugger();
  const ids = new Set();
  const sections = [];
  let current;
  let firstTitle = '';
  const heading = (token) => {
    const text = plain(token.tokens);
    const match = /^(.*?)\s*\{#([\w-]+)\}\s*$/.exec(text);
    if (requireExplicitIds && !match) throw new Error(`markdown: heading needs an explicit {#id}: ${text}`);
    const title = (match ? match[1] : text).trim();
    let id = match?.[2];
    if (id && ids.has(id)) throw new Error(`markdown: duplicate heading id ${id}`);
    if (!id) { do { id = slugs.slug(title) || slugs.slug('section'); } while (ids.has(id)); }
    ids.add(id);
    return { title, id };
  };
  const add = (block) => {
    if (!current) {
      let id = 'overview';
      while (ids.has(id)) id = slugs.slug('overview');
      ids.add(id);
      sections.push(current = { id, title: 'Overview', blocks: [] });
    }
    current.blocks.push(block);
  };
  const convert = (sourceTokens, nearestTitle = 'Table') => {
    const result = [];
    let caption;
    let tabs;
    for (const token of sourceTokens) {
      switch (token.type) {
        case 'space': case 'def': case 'hr': break;
        case 'html': {
          const text = token.raw.trim();
          const table = /^<!--\s*table:\s*(.*?)\s*-->$/.exec(text);
          const open = /^<!--\s*tabs:\s*(.*?)\s*-->$/.exec(text);
          if (table) caption = table[1];
          else if (open) {
            if (tabs) throw new Error('markdown: nested code tabs are not supported');
            tabs = { type: 'tabs', label: open[1], tabs: [] };
          } else if (/^<!--\s*\/tabs\s*-->$/.test(text)) {
            if (!tabs?.tabs.length) throw new Error('markdown: code tabs need labelled fences');
            result.push(tabs); tabs = undefined;
          } else if (!/^<!--[\s\S]*-->$/.test(text)) throw new Error(`markdown: HTML blocks are not supported: ${text.slice(0, 60)}`);
          break;
        }
        case 'code': {
          const code = codeBlock(token);
          if (tabs) {
            if (!code.tab || tabs.tabs.some((tab) => tab.label === code.tab)) throw new Error('markdown: each code tab needs a unique tab="Name"');
            tabs.tabs.push({ label: code.tab, lang: code.lang, code: code.code, ...(code.label ? { codeLabel: code.label } : {}) });
          } else { delete code.tab; result.push(code); }
          break;
        }
        default: {
          if (tabs) throw new Error('markdown: only code fences may sit inside tabs');
          if (token.type === 'paragraph' || token.type === 'text') {
            if (token.type === 'paragraph' && /^Table: [^\n]+$/.test(token.text)) caption = token.text.slice(7).trim();
            else result.push({ type: 'p', children: inline(token.tokens ?? [{ type: 'text', text: token.text }], link) });
          } else if (token.type === 'list') result.push({ type: 'list', ordered: token.ordered, ...(token.ordered && token.start !== 1 ? { start: token.start } : {}), items: token.items.map((item) => convert(item.tokens, nearestTitle)) });
          else if (token.type === 'blockquote') {
            const marker = /^\[!(\w+)\]\s*\n?/.exec(token.text);
            const bold = /^\*\*(Note|Tip|Warning):\*\*\s*/i.exec(token.text);
            const name = (marker?.[1] ?? bold?.[1] ?? 'NOTE').toUpperCase();
            if (!CALLOUTS[name]) throw new Error(`markdown: unknown callout [!${name}]`);
            const content = token.text.slice((marker ?? bold)?.[0].length ?? 0);
            const nestedLexer = new Lexer({ gfm: true });
            nestedLexer.tokens.links = lexer.tokens.links;
            const nested = nestedLexer.lex(content);
            result.push({ type: 'callout', kind: CALLOUTS[name], blocks: convert(nested, nearestTitle) });
          } else if (token.type === 'table') {
            result.push({ type: 'table', caption: caption ?? nearestTitle, heads: token.header.map((cell) => plain(cell.tokens)), rows: token.rows.map((row) => row.map((cell) => inline(cell.tokens, link))) });
            caption = undefined;
          } else if (token.type === 'heading') {
            if (token.depth < 3) throw new Error('markdown: section headings cannot sit inside a block');
            result.push({ type: 'h3', ...heading(token) });
          } else throw new Error(`markdown: ${token.type} is not supported`);
        }
      }
    }
    if (tabs) throw new Error('markdown: unclosed code tabs');
    return result;
  };
  let pending = [];
  const flush = () => { if (pending.length) { for (const block of convert(pending, current?.title)) add(block); pending = []; } };
  for (const token of tokens) {
    if (token.type === 'heading' && token.depth <= 2) {
      flush();
      if (token.depth === 1) { firstTitle ||= plain(token.tokens); continue; }
      const head = heading(token);
      sections.push(current = { ...head, blocks: [] });
    } else pending.push(token);
  }
  flush();
  return {
    slug: fields.slug ?? '', group: fields.group ?? '', title: fields.title ?? firstTitle, lead: fields.lead ?? '',
    description: fields.description ?? fields.lead ?? '', ...(fields.order !== undefined ? { order: Number(fields.order) } : {}), sections,
  };
}

const interactive = (block) => block.type === 'code' || block.type === 'tabs' || (block.type === 'list' && block.items.some((item) => item.some(interactive))) || (block.type === 'callout' && block.blocks.some(interactive));
/** Only content that needs script crosses into the browser build. */
export function splitDoc(page) {
  const sections = page.sections.map((section) => {
    const items = [{ type: 'prose', blocks: [] }];
    for (const block of section.blocks) {
      if (interactive(block)) items.push(block);
      else {
        if (items.at(-1).type !== 'prose') items.push({ type: 'prose', blocks: [] });
        items.at(-1).blocks.push(block);
      }
    }
    return { id: section.id, title: section.title, items };
  });
  const contents = page.sections.flatMap((section) => [{ id: section.id, label: section.title, level: 2 }, ...section.blocks.filter((block) => block.type === 'h3').map((block) => ({ id: block.id, label: block.title, level: 3 }))]);
  const { sections: _, ...metadata } = page;
  return {
    requiresRich: sections.some(({ items }) => items.some((item) => item.type !== 'prose' && item.type !== 'code' && item.type !== 'tabs')),
    full: { ...metadata, sections },
    client: { ...metadata, contents, sections: sections.map(({ id, items }) => ({ id, items: items.map((item) => item.type === 'prose' ? { type: 'prose' } : item) })) },
  };
}
