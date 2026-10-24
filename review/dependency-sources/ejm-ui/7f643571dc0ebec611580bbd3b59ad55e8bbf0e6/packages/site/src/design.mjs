// Node-only ownership checks for sites. Shared packages own the visual recipes.
import { createRequire } from 'node:module';
import { existsSync, readFileSync, readdirSync, realpathSync, statSync } from 'node:fs';
import { dirname, extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseSync } from 'vite';
import postcss from 'postcss';

const require = createRequire(import.meta.url);
const gallery = resolve(dirname(fileURLToPath(import.meta.url)), '../../../apps/gallery');
const sourceExtensions = new Set(['.js', '.jsx', '.ts', '.tsx', '.css', '.scss', '.sass', '.less', '.styl']);
const scriptExtensions = new Set(['.js', '.jsx', '.ts', '.tsx']);
const retired = {
  Hero: new Set(['motion', 'size', 'tone']),
  ProductHero: new Set(['motion', 'size', 'tone']),
  ProductHeroStage: new Set(['motion', 'size', 'tone']),
  SectionHead: new Set(['size', 'layout']),
  Brand: new Set(['size']),
  Header: new Set(['logo']),
  Footer: new Set(['brand']),
};
const sharedSource = source => source === '@ejm/ui' || source.startsWith('@ejm/ui/') || source === '@ejm/demos' || source.startsWith('@ejm/demos/');
const under = (parent, file) => { const path = relative(parent, file); return path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path); };
const actualPath = path => existsSync(path) ? realpathSync(path) : resolve(path);
const literal = node => node?.type === 'Literal' ? node.value : undefined;
const propertyName = node => node?.type === 'Identifier' || node?.type === 'JSXIdentifier' ? node.name : literal(node);
const expression = node => node?.type === 'JSXExpressionContainer' ? node.expression : node;

function walk(node, visit) {
  if (!node || typeof node !== 'object') return;
  if (typeof node.type === 'string') visit(node);
  for (const [key, value] of Object.entries(node)) {
    if (key === 'comments' || key === 'errors') continue;
    if (Array.isArray(value)) for (const child of value) walk(child, visit);
    else if (value && typeof value === 'object') walk(value, visit);
  }
}

function filesIn(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const file = join(directory, entry.name);
    if (entry.isDirectory()) return filesIn(file);
    return entry.isFile() && sourceExtensions.has(extname(file)) ? [file] : [];
  });
}

function cssImport(request) {
  if (!/^@ejm\/(?:ui|tokens|demos)\/[a-zA-Z0-9_./-]+\.css$/.test(request)) return false;
  try { require.resolve(request); return true; } catch { return false; }
}

function cssIssues(file, source, report) {
  const root = postcss.parse(source, { from: file });
  for (const node of root.nodes) {
    if (node.type === 'comment') continue;
    if (node.type === 'atrule' && !node.nodes) {
      const quoted = /^(['"])([^'"]+)\1(?:\s+source\(none\))?$/.exec(node.params);
      if (node.name === 'import' && quoted && cssImport(quoted[2])) continue;
      if (node.name === 'source' && /^not\s+(['"])\.\.\/content\1$/.test(node.params)) continue;
    }
    report(node.source.start.offset, 'Consumer CSS may only import public @ejm styles or exclude ../content from the content scan.');
  }
}

function scriptIssues(file, source, report, sourceRoot) {
  const parsed = parseSync(file, source, { lang: extname(file) === '.ts' ? 'ts' : extname(file) === '.js' ? 'js' : 'tsx' });
  if (parsed.errors.length) {
    for (const error of parsed.errors) report(error.start ?? 0, `Cannot check invalid source: ${error.message}`);
    return;
  }
  const imports = new Map();
  const values = new Map();
  const checkStyleImport = (from, node) => {
    if (typeof from !== 'string' || !/\.(?:css|scss|sass|less|styl)(?:$|[?#])/.test(from)) return;
    if (from.startsWith('@')) {
      if (!cssImport(from)) report(node.start, 'Import styles only through a public @ejm CSS export.');
    } else if (!from.startsWith('.') || extname(from) !== '.css' || !under(sourceRoot, resolve(dirname(file), from))) {
      report(node.start, 'Import only a CSS entry inside src or a public @ejm CSS export.');
    }
  };
  for (const node of parsed.program.body) {
    if (node.type !== 'ImportDeclaration' || node.importKind === 'type') continue;
    const from = literal(node.source);
    checkStyleImport(from, node);
    for (const specifier of node.specifiers) {
      if (specifier.importKind === 'type') continue;
      imports.set(specifier.local.name, {
        source: from,
        name: specifier.type === 'ImportNamespaceSpecifier' ? '*' : propertyName(specifier.imported) ?? 'default',
      });
    }
  }
  walk(parsed.program, node => {
    if (node.type === 'ImportExpression') checkStyleImport(literal(node.source), node);
    if (node.type === 'VariableDeclarator' && node.id?.type === 'Identifier') values.set(node.id.name, node.init);
  });
  const binding = node => {
    if (node?.type === 'JSXIdentifier' || node?.type === 'Identifier') return imports.get(node.name);
    if (node?.type === 'JSXMemberExpression' || node?.type === 'MemberExpression') {
      const object = imports.get(node.object?.name);
      if (object?.name === '*' && !node.computed) return { source: object.source, name: propertyName(node.property) };
    }
    return undefined;
  };
  const objectValue = (node, seen = new Set()) => {
    node = expression(node);
    if (node?.type === 'Identifier' && !seen.has(node.name)) {
      seen.add(node.name);
      return objectValue(values.get(node.name), seen);
    }
    return node?.type === 'ObjectExpression' ? node : undefined;
  };
  const identityProps = (node, old, label, seen = new Set()) => {
    const object = objectValue(node);
    if (!object || seen.has(object)) return;
    seen.add(object);
    for (const property of object.properties) {
      if (property.type === 'SpreadElement') {
        if (objectValue(property.argument)) identityProps(property.argument, old, label, seen);
        else report(property.start, `${label} cannot spread opaque props. Supply the shared identity contract.`);
      } else if ([old, 'className', 'style'].includes(propertyName(property.key))) {
        report(property.start, `${label}.${propertyName(property.key)} is no longer supported. Supply identity data to the shared component.`);
      }
    }
  };
  walk(parsed.program, node => {
    if (node.type !== 'JSXOpeningElement') return;
    const component = binding(node.name);
    const tag = node.name.type === 'JSXIdentifier' ? node.name.name : component?.name;
    if (tag === 'style') report(node.start, 'Consumer <style> elements are not allowed.');
    if (tag === 'link' && node.attributes.some(attribute => attribute.type === 'JSXAttribute' && propertyName(attribute.name) === 'rel' && literal(expression(attribute.value)) === 'stylesheet')) {
      report(node.start, 'Consumer stylesheet links are not allowed. Import public @ejm styles through the CSS entry.');
    }
    for (const attribute of node.attributes) {
      if (attribute.type === 'JSXSpreadAttribute') {
        const call = attribute.argument;
        const helper = call?.type === 'CallExpression' ? binding(call.callee) : undefined;
        if (helper?.name !== 'docNavigation' || !['@ejm/ui/docs', '@ejm/ui/docs/DocsPage'].includes(helper.source)) {
          report(attribute.start, 'Opaque JSX props can override shared design. Pass named content and state props.');
        }
        continue;
      }
      const name = propertyName(attribute.name);
      const structural = component?.source === '@ejm/ssg' && component.name === 'Static' && name === 'className' && literal(attribute.value) === 'contents';
      if (name === 'dangerouslySetInnerHTML') report(attribute.start, 'Consumer raw markup is not allowed. Use the shared content renderer or Static boundary.');
      if (name === 'style' || ((name === 'className' || name === 'class') && !structural)) {
        report(attribute.start, `Consumer ${name} overrides are not allowed. Use a shared component for this layout or control.`);
      }
      if (component && sharedSource(component.source) && retired[component.name]?.has(name)) {
        report(attribute.start, `${component.name}.${name} is no longer supported. Its visual recipe is owned by ejm-ui.`);
      }
      if (component?.name === 'SiteLayout' && sharedSource(component.source)) {
        if (name === 'header') identityProps(attribute.value, 'logo', 'SiteLayout.header');
        if (name === 'footer') identityProps(attribute.value, 'brand', 'SiteLayout.footer');
      }
    }
  });
}

/** Internal Node contract. Every consumer uses it through ejmSite, with no opt-out. */
export function siteDesignGuard(root) {
  const site = actualPath(root);
  const sourceRoot = join(site, 'src');
  const libraryGallery = site === actualPath(gallery);
  const cache = new Map();
  const owns = file => under(sourceRoot, resolve(file)) && sourceExtensions.has(extname(file));
  return {
    owns,
    invalidate(file) { cache.delete(resolve(file)); },
    validate() {
      const started = performance.now();
      const files = libraryGallery ? [] : filesIn(sourceRoot);
      const issues = [];
      for (const file of files) {
        const stat = statSync(file);
        let cached = cache.get(file);
        if (!cached || cached.time !== stat.mtimeMs || cached.size !== stat.size) {
          const source = readFileSync(file, 'utf8');
          const errors = [];
          const report = (offset, message) => {
            const prefix = source.slice(0, offset);
            const lines = prefix.split('\n');
            errors.push(`${relative(site, file)}:${lines.length}:${lines.at(-1).length + 1} ${message}`);
          };
          try {
            if (extname(file) === '.css') cssIssues(file, source, report);
            else if (scriptExtensions.has(extname(file))) scriptIssues(file, source, report, sourceRoot);
            else report(0, 'Consumer styles must use the shared CSS entry points.');
          } catch (error) {
            report(error.offset ?? 0, `Cannot check source: ${error.reason ?? error.message}`);
          }
          cached = { time: stat.mtimeMs, size: stat.size, errors };
          cache.set(file, cached);
        }
        issues.push(...cached.errors);
      }
      const currentFiles = new Set(files);
      for (const file of cache.keys()) if (!currentFiles.has(file)) cache.delete(file);
      if (issues.length) {
        throw new Error(`[ejm-site:design] ${issues.join('\n[ejm-site:design] ')}\nExtend the public ejm-ui contract for missing patterns, then rebuild the site. See @ejm/site/README.md, Design ownership.`);
      }
      return { files, durationMs: performance.now() - started };
    },
  };
}
