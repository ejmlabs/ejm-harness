// Original chess artwork. Render text with the owned Manrope font, then outline it.
// Requires librsvg and Python Pillow, as does the existing favicon generator.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const tokens = join(root, 'packages/tokens');
const directory = join(tokens, 'assets/projects/chess');
const colors = JSON.parse(readFileSync(join(tokens, 'assets/brand/palette.json'), 'utf8')).colors;
const renderer = process.env.RSVG_CONVERT || 'rsvg-convert';
const python = process.env.PYTHON || ['python3', '/usr/bin/python3'].find(candidate => {
  try { execFileSync(candidate, ['-c', 'from PIL import Image'], { stdio: 'ignore' }); return true; } catch { return false; }
});
if (!python) throw new Error('Pillow is required. Set PYTHON to a Python with PIL installed.');
const stage = mkdtempSync(join(tmpdir(), 'ejm-chess-brand-'));
// Original two-ribbon knight. Design coordinates use the family's 256-unit box.
// Ribbon width 36, round terminals and one 82.8-unit curved taper. The filled
// paths are the canonical artwork; rendering needs no retained-kit code.
const chessMark = {
  "box": 256,
  "extent": [
    180.9315045656514,
    212.6582682245997
  ],
  "paths": [
    "M45.14 103.58A147 147 0 0 0 98.96 33.45A18 18 0 0 1 132.79 45.76A183 183 0 0 1 65.79 133.07A18 18 0 0 1 45.14 103.58Z",
    "M94.47 198.33L154.47 198.33A28 28 0 0 0 182.47 170.33A152 152 0 0 0 175.75 125.65C175.62 125.23 175.24 123.96 174.99 123.12C174.74 122.27 174.5 121.42 174.26 120.57C174.02 119.72 173.79 118.87 173.56 118.01C173.33 117.16 173.1 116.3 172.88 115.43C172.66 114.57 172.44 113.7 172.23 112.83C172.01 111.96 171.8 111.09 171.59 110.21C171.38 109.33 171.18 108.45 170.97 107.56C170.77 106.68 170.57 105.79 170.36 104.89C170.16 104 169.96 103.1 169.76 102.2C169.56 101.3 169.36 100.39 169.16 99.48C168.95 98.57 168.75 97.65 168.55 96.73C168.34 95.82 168.14 94.89 167.93 93.97C167.72 93.04 167.5 92.11 167.29 91.18C167.07 90.24 166.85 89.31 166.62 88.37C166.39 87.43 166.16 86.49 165.92 85.54C165.68 84.6 165.44 83.65 165.19 82.71C164.93 81.76 164.67 80.81 164.41 79.86C164.14 78.91 163.86 77.96 163.57 77.01C163.28 76.06 162.99 75.11 162.68 74.16C162.37 73.22 162.05 72.27 161.72 71.32C161.39 70.38 161.05 69.44 160.69 68.5C160.33 67.56 159.96 66.63 159.58 65.7C159.19 64.77 158.79 63.85 158.38 62.93C157.96 62.02 157.53 61.11 157.08 60.21C156.63 59.31 156.17 58.42 155.68 57.54C155.2 56.67 154.7 55.8 154.17 54.95C153.64 54.1 153.1 53.26 152.52 52.45C151.93 51.65 150.98 50.51 150.67 50.12C151.06 50.44 152.19 51.4 152.98 52.01C153.76 52.62 154.58 53.21 155.4 53.8C156.21 54.4 157.04 54.98 157.88 55.57C158.71 56.16 159.55 56.74 160.4 57.33C161.24 57.92 162.09 58.51 162.94 59.1C163.8 59.69 164.65 60.29 165.51 60.9C166.36 61.5 167.22 62.11 168.08 62.72C168.94 63.34 169.8 63.96 170.65 64.59C171.51 65.22 172.36 65.86 173.21 66.5C174.06 67.15 174.91 67.8 175.76 68.47C176.6 69.13 177.44 69.81 178.28 70.49C179.11 71.18 179.94 71.87 180.76 72.58C181.58 73.28 182.4 74 183.21 74.72C184.02 75.45 184.82 76.19 185.61 76.94C186.4 77.69 187.18 78.45 187.95 79.22C188.72 80 189.48 80.78 190.22 81.58C190.97 82.38 191.71 83.19 192.43 84.01C193.15 84.83 193.86 85.66 194.55 86.5C195.25 87.35 195.93 88.21 196.6 89.07C197.26 89.94 197.91 90.82 198.54 91.71C199.18 92.61 199.79 93.51 200.39 94.42C200.99 95.34 201.58 96.26 202.14 97.2C202.7 98.13 203.25 99.08 203.78 100.04C204.3 100.99 204.81 101.96 205.3 102.94C205.79 103.91 206.25 104.9 206.7 105.89C207.15 106.89 207.57 107.89 207.98 108.9C208.39 109.92 208.77 110.94 209.13 111.96C209.5 112.99 209.99 114.55 210.16 115.07A188 188 0 0 1 218.47 170.33A64 64 0 0 1 154.47 234.33L94.47 234.33A18 18 0 0 1 94.47 198.33Z"
  ]
};
const art = color => `<g transform="scale(.25)">${chessMark.paths.map(path => `<path fill="${color}" d="${path}"/>`).join('')}</g>`;
const iconArt = (background, foreground, fill) => {
  const scale = 64 * fill / (Math.max(...chessMark.extent) / 4);
  const inset = (64 - 64 * scale) / 2;
  return `<path fill="${background}" d="M0 0H64V64H0Z"/><g transform="translate(${inset} ${inset}) scale(${scale})">${art(foreground)}</g>`;
};
const svg = (width, height, label, content) => `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="${label}"><title>${label}</title>${content}</svg>\n`;
const files = new Map();
const add = (name, content) => { writeFileSync(join(stage, name), content); files.set(name, readFileSync(join(stage, name))); };
const render = (name, source, size, format = 'png') => {
  const output = execFileSync(renderer, ['--format', format, ...(size ? ['--width', String(size)] : []), join(stage, source)], { env: renderEnv });
  add(name, output);
};
let renderEnv;
try {
  // Isolate font lookup to the included source. Generated SVG wordmarks contain paths.
  const config = join(stage, 'fonts.conf');
  writeFileSync(config, `<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd"><fontconfig><dir>${join(tokens, 'assets/fonts')}</dir><cachedir>${stage}/font-cache</cachedir></fontconfig>`);
  renderEnv = { ...process.env, FONTCONFIG_FILE: config };
  const match = execFileSync('fc-match', ['--format', '%{family}', 'Manrope'], { env: renderEnv }).toString();
  if (!match.includes('Manrope')) throw new Error('The owned Manrope font could not be resolved.');
  for (const [name, color] of [['ink', colors.ink], ['paper', colors.paper], ['sage', colors.sage]]) {
    add(`mark-${name}.svg`, svg(64, 64, 'Chess by ejmlabs', art(color)));
  }
  for (const [name, color] of [['ink', colors.ink], ['paper', colors.paper], ['primary', colors.ink]]) {
    const source = `logo-${name}-source.svg`;
    writeFileSync(join(stage, source), svg(218, 64, 'Chess by ejmlabs', `${art(name === 'primary' ? colors.sage : color)}<text x="76" y="47" font-family="Manrope" font-size="46" font-weight="650" letter-spacing="-2" fill="${color}">chess</text>`));
    render(`logo-${name}.svg`, source, undefined, 'svg');
  }
  add('icon-sage.svg', svg(64, 64, 'Chess by ejmlabs', iconArt(colors.sage, colors.ink, 2 / 3)));
  add('icon-forest.svg', svg(64, 64, 'Chess by ejmlabs', iconArt(colors.forest, colors.paper, 2 / 3)));
  // Match the family favicon cut: its longest side fills 80 percent of the canvas.
  add('favicon.svg', svg(64, 64, 'Chess by ejmlabs', iconArt(colors.sage, colors.ink, .8)));
  for (const [name, size] of [['favicon-16.png', 16], ['favicon-24.png', 24], ['favicon-32.png', 32], ['favicon-48.png', 48], ['apple-touch-icon.png', 180], ['android-chrome-192.png', 192], ['android-chrome-512.png', 512]]) render(name, 'favicon.svg', size);
  execFileSync(python, ['-c', 'from PIL import Image; import sys; Image.open(sys.argv[1]).save(sys.argv[2], format="ICO", sizes=[(16,16),(32,32),(48,48)])', join(stage, 'android-chrome-512.png'), join(stage, 'favicon.ico')]);
  files.set('favicon.ico', readFileSync(join(stage, 'favicon.ico')));
  add('site.webmanifest', JSON.stringify({ id: '/', name: 'Chess by ejmlabs', short_name: 'Chess', start_url: '/', scope: '/', display: 'standalone', background_color: colors.paper, theme_color: colors.sage, icons: [192, 512].map(size => ({ src: `android-chrome-${size}.png`, sizes: `${size}x${size}`, type: 'image/png', purpose: 'any' })) }, null, 2) + '\n');
  const socialSource = 'social-source.svg';
  writeFileSync(join(stage, socialSource), svg(1200, 630, 'Chess by ejmlabs', `<path fill="${colors.paper}" d="M0 0H1200V630H0Z"/><g transform="translate(90 140) scale(4.5)">${art(colors.forest)}</g><text x="450" y="340" font-family="Manrope" font-size="142" font-weight="650" letter-spacing="-6" fill="${colors.ink}">chess</text><text x="455" y="410" font-family="Manrope" font-size="30" font-weight="500" fill="${colors.forest}">by ejmlabs</text>`));
  render('social-card.svg', socialSource, undefined, 'svg');
  render('social-card.png', 'social-card.svg', 1200);
  const provenance = join(tokens, 'assets/PROVENANCE.md');
  const text = readFileSync(provenance, 'utf8');
  const rows = [...files].sort(([a], [b]) => a.localeCompare(b)).map(([name, buffer]) => `| \`assets/projects/chess/${name}\` | \`generated:chess-assets/${name}\` | \`${createHash('sha256').update(buffer).digest('hex')}\` |`);
  const clean = text.split('\n').filter(line => !line.startsWith('| `assets/projects/chess/')).join('\n');
  const marker = '\n## Moved and then edited';
  if (!clean.includes(marker)) throw new Error('Provenance table insertion point is missing.');
  const updated = clean.replace(marker, rows.join('\n') + '\n' + marker);
  if (process.argv.includes('--check')) {
    for (const [name, buffer] of files) if (!existsSync(join(directory, name)) || !readFileSync(join(directory, name)).equals(buffer)) throw new Error(`Chess asset differs: ${name}`);
    const extra = readdirSync(directory).filter(name => !files.has(name));
    if (extra.length) throw new Error(`Unexpected chess assets: ${extra.join(', ')}`);
    if (text !== updated) throw new Error('Chess asset provenance differs.');
  } else {
    mkdirSync(directory, { recursive: true });
    for (const [name, buffer] of files) writeFileSync(join(directory, name), buffer);
    writeFileSync(provenance, updated);
  }
  console.log(`Chess assets ${process.argv.includes('--check') ? 'verified' : 'generated'}: ${files.size} files.`);
} finally { rmSync(stage, { recursive: true, force: true }); }
