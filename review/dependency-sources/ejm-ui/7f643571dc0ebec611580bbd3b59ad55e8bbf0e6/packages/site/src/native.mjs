/** Node-only checks for applications that compose the public native HTML profile. */
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { extname, join, relative, resolve } from 'node:path';
import { parseSync } from 'vite';
import { parse as parseHtml } from 'parse5';
import postcss from 'postcss';

const styles = new Set(['.css', '.scss', '.sass', '.less', '.styl']);
const extensions = new Set(['.html', '.js', '.mjs', '.ts', ...styles]);
const recipes = new Set(['skip', 'header', 'header-inner', 'brand', 'brand-artwork', 'nav', 'nav-list', 'menu', 'menu-list', 'page', 'heading', 'eyebrow', 'lede', 'stack', 'group', 'section', 'form-field', 'actions', 'action-row', 'list', 'row', 'muted', 'helper', 'caption', 'notice', 'status', 'footer', 'checkbox', 'check', 'metadata', 'fingerprint', 'setup', 'workspace', 'workspace-header', 'sidebar', 'session', 'session-header', 'transcript', 'empty', 'message', 'message-label', 'composer', 'compose-title', 'button', 'field']);
const states = {
  'data-ejm': recipes,
  'data-theme': new Set(['light', 'dark']),
  'data-tone': new Set(['muted', 'success', 'warning', 'danger']),
  'data-motion': new Set(['page', 'enter', 'arrive']),
  'data-variant': new Set(['primary', 'secondary', 'ghost', 'link', 'danger']),
  'data-size': new Set(['sm', 'md', 'lg', 'icon']),
  'data-density': new Set(['comfortable', 'compact']),
};
const literal = node => node?.type === 'Literal' ? node.value : undefined;
const property = node => node?.computed ? literal(node.property) : node?.property?.name;
const filesIn = directory => !existsSync(directory) ? [] : readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
  const file = join(directory, entry.name);
  return entry.isDirectory() ? ['node_modules', 'dist', 'test-results'].includes(entry.name) || entry.name.startsWith('.') ? [] : filesIn(file) : entry.isFile() && extensions.has(extname(file)) && !/\.(?:test|spec|d)\.[^.]+$/.test(file) ? [file] : [];
});
function walk(node, visit) {
  if (!node || typeof node !== 'object') return;
  if (typeof node.type === 'string') visit(node);
  for (const [key, value] of Object.entries(node)) {
    if (key === 'comments' || key === 'errors') continue;
    if (Array.isArray(value)) for (const child of value) walk(child, visit);
    else if (value && typeof value === 'object') walk(value, visit);
  }
}
function attributeIssue(name, value) {
  if (['class', 'classname', 'style'].includes(name.toLowerCase())) return `Consumer ${name} is not allowed.`;
  if (states[name] && !states[name].has(value)) return `Unknown shared ${name} value: ${JSON.stringify(value)}.`;
}
function htmlIssues(source, stylesheetHref, report) {
  const document = parseHtml(source, { sourceCodeLocationInfo: true });
  const visit = node => {
    const offset = node.sourceCodeLocation?.startOffset ?? 0;
    if (node.tagName === 'style') report(offset, 'Consumer style elements are not allowed.');
    if (['svg', 'canvas', 'font', 'center'].includes(node.tagName)) report(offset, 'Consumer drawings and presentational elements are not allowed. Use public shared artwork or a shared view.');
    for (const attribute of node.attrs ?? []) {
      const issue = attributeIssue(attribute.name, attribute.value) ?? (['align', 'bgcolor', 'background', 'border', 'cellpadding', 'cellspacing', 'color'].includes(attribute.name) || ['width', 'height'].includes(attribute.name) && node.tagName !== 'img' ? `Consumer visual attribute ${attribute.name} is not allowed.` : undefined);
      if (issue) report(node.sourceCodeLocation?.attrs?.[attribute.name]?.startOffset ?? offset, issue);
    }
    const attrs = Object.fromEntries((node.attrs ?? []).map(attribute => [attribute.name, attribute.value]));
    if (node.tagName === 'button') {
      if (attrs['data-ejm'] !== 'button') report(offset, 'Native buttons require data-ejm="button".');
      if (!['button', 'submit', 'reset'].includes(attrs.type?.toLowerCase())) report(offset, 'Native buttons require an explicit button, submit or reset type.');
    }
    if (['input', 'select', 'textarea'].includes(node.tagName) && !(node.tagName === 'input' && attrs.type?.toLowerCase() === 'hidden')) {
      if (node.tagName === 'input' && attrs.type?.toLowerCase() === 'checkbox') {
        let label = node.parentNode;
        while (label && label.tagName !== 'label') label = label.parentNode;
        if (!label?.attrs?.some(attribute => attribute.name === 'data-ejm' && ['checkbox', 'check'].includes(attribute.value))) report(offset, 'Native checkboxes require a wrapping shared checkbox or check label.');
      } else if (attrs['data-ejm'] !== 'field') report(offset, 'Native fields require data-ejm="field".');
    }
    if (node.tagName === 'link' && attrs.rel?.toLowerCase().split(/\s+/).includes('stylesheet') && attrs.href !== stylesheetHref) {
      report(offset, `Native stylesheets must use the shared asset ${stylesheetHref}.`);
    }
    for (const child of node.childNodes ?? []) visit(child);
    if (node.content) visit(node.content);
  };
  visit(document);
}
function cssIssues(source, stylesheetHref, report) {
  const css = postcss.parse(source);
  for (const node of css.nodes) {
    if (node.type === 'comment') continue;
    const match = node.type === 'atrule' && node.name === 'import' && node.params.match(/^(?:url\(\s*)?["']([^"']+)["']\s*\)?$/);
    if (match && [stylesheetHref, '@ejm/ui/static.css'].includes(match[1])) continue;
    report(node.source?.start?.offset ?? 0, 'Consumer CSS may only import the public native HTML stylesheet.');
  }
}
function scriptIssues(file, source, stylesheetHref, report) {
  const parsed = parseSync(file, source, { lang: extname(file) === '.ts' ? 'ts' : 'js' });
  for (const error of parsed.errors) report(error.start ?? 0, `Cannot check invalid source: ${error.message}`);
  if (parsed.errors.length) return;
  walk(parsed.program, node => {
    if (node.type === 'Literal' && typeof node.value === 'string' && /<\/?[a-z!]/i.test(node.value)) {
      htmlIssues(node.value, stylesheetHref, (offset, message) => report(node.start + 1 + offset, message));
    }
    if (node.type === 'TemplateLiteral') {
      const text = node.quasis.map(part => part.value.cooked ?? part.value.raw).join('ejm-content');
      if (/<\/?[a-z!]/i.test(text)) htmlIssues(text, stylesheetHref, (offset, message) => report(node.start + 1 + offset, message));
    }
    if (node.type === 'TaggedTemplateExpression' && node.tag.type === 'MemberExpression' && node.tag.object.name === 'String' && property(node.tag) === 'raw' && node.quasi.expressions.length === 0) {
      const embedded = node.quasi.quasis[0].value.raw;
      if (!/<\/?[a-z!]/i.test(embedded)) scriptIssues(file, embedded, stylesheetHref, (offset, message) => report(node.quasi.start + 1 + offset, message));
    }
    if (node.type === 'ImportDeclaration' || node.type === 'ImportExpression') {
      const request = literal(node.source);
      if (/\.(?:css|scss|sass|less|styl)(?:$|[?#])/.test(request ?? '') && request !== '@ejm/ui/static.css') report(node.start, 'Import only the public native HTML stylesheet.');
    }
    if (node.type === 'MemberExpression' && ['className', 'classList', 'style', 'adoptedStyleSheets', 'innerHTML', 'outerHTML'].includes(property(node))) {
      report(node.start, `Consumer DOM ${property(node)} is not allowed. Use a shared data-ejm recipe and text nodes.`);
    }
    if (node.type === 'AssignmentExpression' && node.left.type === 'MemberExpression') {
      const key = property(node.left);
      if (node.left.object.type === 'MemberExpression' && property(node.left.object) === 'dataset') {
        const issue = attributeIssue(`data-${key}`, literal(node.right));
        if (issue && node.right.type === 'Literal') report(node.start, issue);
      }
      if (key === 'rel' && literal(node.right) === 'stylesheet') report(node.start, 'Consumer stylesheet insertion is not allowed.');
    }
    if (node.type !== 'CallExpression' || node.callee.type !== 'MemberExpression') return;
    const method = property(node.callee);
    if (['insertAdjacentHTML', 'insertRule', 'replaceSync', 'write', 'writeln'].includes(method)) report(node.start, 'Consumer raw markup or stylesheet insertion is not allowed.');
    if (method === 'createElement' && ['style', 'link'].includes(literal(node.arguments[0]))) report(node.start, 'Consumer style or stylesheet elements are not allowed.');
    if (method === 'setAttribute') {
      const name = literal(node.arguments[0]);
      if (typeof name !== 'string') report(node.start, 'Opaque DOM attributes may override the shared design. Use a named semantic attribute.');
      else {
        const issue = attributeIssue(name, literal(node.arguments[1]));
        if (issue && (!states[name] || node.arguments[1]?.type === 'Literal')) report(node.start, issue);
      }
    }
    if (method === 'import' && /\.css(?:$|[?#])/.test(literal(node.arguments[0]) ?? '')) report(node.start, 'Import only the public native HTML stylesheet.');
  });
}

/** Check authored source and each generated document before copying native assets. */
export function checkNativeDesign({ root, source = ['src', 'web'], stylesheetHref, documents = [] }) {
  if (typeof stylesheetHref !== 'string' || !stylesheetHref.startsWith('/') || stylesheetHref.startsWith('//')) throw new Error('A same-origin shared stylesheet asset path is required.');
  const base = resolve(root);
  const files = [...new Set(source.flatMap(directory => filesIn(resolve(base, directory))))];
  const issues = [];
  const check = (file, contents, kind) => {
    const report = (offset, message) => {
      const lines = contents.slice(0, offset).split('\n');
      issues.push(`${file}:${lines.length}:${lines.at(-1).length + 1} ${message}`);
    };
    try {
      if (kind === '.html') htmlIssues(contents, stylesheetHref, report);
      else if (styles.has(kind)) cssIssues(contents, stylesheetHref, report);
      else scriptIssues(file, contents, stylesheetHref, report);
    } catch (error) { report(error.offset ?? 0, `Cannot check source: ${error.reason ?? error.message}`); }
  };
  for (const file of files) check(relative(base, file), readFileSync(file, 'utf8'), extname(file));
  for (const document of documents) check(document.name, document.html, '.html');
  if (issues.length) throw new Error(`[ejm-native:design] ${issues.join('\n[ejm-native:design] ')}\nExtend the public ejm-ui native HTML contract for missing patterns, then rebuild. See @ejm/site/native-html.md.`);
  return { files, documents: documents.length };
}
