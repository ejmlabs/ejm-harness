/**
 * A small syntax tokenizer for the documentation code blocks. It returns the code split into runs, each with a role
 * that maps to a `syn-*` window color (all gated at 4.5:1 on the code ground). Joining the runs gives back the input
 * exactly, so copying never depends on the spans. It runs on the server too, so highlighted code needs no script.
 * Roles are short codes: k keyword, s string, c comment, n number, t type, f function, m macro, p property, v constant.
 */
export type Role = 'k' | 's' | 'c' | 'n' | 't' | 'f' | 'm' | 'p' | 'v';
export interface Run {
  role?: Role;
  text: string;
}
export type Rule = [Role, RegExp];
/** Languages by name, each a list of rules tried in order at every position. */
export type Grammars = Record<string, Rule[]>;

/** @internal for highlight-more.ts */
export const words = (list: string) => new RegExp(`\\b(?:${list.split(' ').join('|')})\\b`, 'y');
/** @internal for highlight-more.ts */
export const sticky = (re: RegExp) => new RegExp(re.source, `${re.flags}y`);
const str = sticky(/"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`(?:[^`\\]|\\.)*`/);
/** @internal for highlight-more.ts */
export const num = sticky(/\b0x[\da-f]+\b|\b\d[\d_]*(?:\.\d+)?(?:[a-z%]{0,3})\b/i);
const slashComment = sticky(/\/\/[^\n]*|\/\*[\s\S]*?\*\//);

const ts: Rule[] = [
  ['c', slashComment],
  ['s', str],
  ['n', num],
  ['p', sticky(/[a-z_$][\w$]*(?=\??:)/i)],
  ['k', words('import export from default const let var function return if else for while switch case break continue new class extends implements interface type enum as typeof keyof in of await async yield throw try catch finally void this satisfies readonly declare namespace')],
  ['v', words('true false null undefined')],
  ['t', sticky(/\b[A-Z][\w$]*/)],
  ['f', sticky(/[a-z_$][\w$]*(?=\()/i)],
];
const rust: Rule[] = [
  ['c', slashComment],
  ['s', sticky(/"(?:[^"\\]|\\[\s\S])*"|'(?:[^'\\\n]|\\.)'/)],
  ['n', num],
  ['k', words('use mod pub fn let mut const static struct enum impl trait for in while loop if else match return self Self crate super as where async await move ref dyn type unsafe')],
  ['v', words('true false None Some Ok Err')],
  ['m', sticky(/\b[a-z_]\w*!/)],
  ['t', sticky(/\b[A-Z]\w*/)],
  ['f', sticky(/\b[a-z_]\w*(?=\s*[(<])/)],
];
const css: Rule[] = [
  ['c', sticky(/\/\*[\s\S]*?\*\//)],
  ['s', str],
  ['k', sticky(/@[\w-]+/)],
  ['p', sticky(/--[\w-]+|[a-z-]+(?=\s*:\s)/i)],
  ['v', sticky(/#[\da-f]{3,8}\b/i)],
  ['n', sticky(/(?<![\w-])-?\d*\.?\d+(?:px|rem|em|ch|vw|vh|ms|s|%|deg)?\b/)],
  ['f', sticky(/[a-z-]+(?=\()/i)],
];
const sh: Rule[] = [
  ['c', sticky(/#[^\n]*/)],
  ['s', sticky(/"(?:[^"\\]|\\.)*"|'[^']*'/)],
  ['v', sticky(/\$\{?\w+\}?/)],
  ['p', sticky(/(?<=\s)--?[a-z][\w-]*/i)],
  ['f', sticky(/(?<=^|\n|&&\s|\|\|?\s|;\s)[\w./-]+/)],
];
const json: Rule[] = [
  ['p', sticky(/"(?:[^"\\\n]|\\.)*"(?=\s*:)/)],
  ['s', str],
  ['v', words('true false null')],
  ['n', num],
];
const html: Rule[] = [
  ['c', sticky(/<!--[\s\S]*?-->/)],
  ['s', str],
  ['k', sticky(/<\/?[a-z][\w-]*|\/?>/i)],
  ['p', sticky(/[a-z-]+(?==)/i)],
];

const grammars: Grammars = { ts, tsx: ts, js: ts, jsx: ts, rust, rs: rust, css, sh, bash: sh, shell: sh, json, html };
const text = 'text';
const word = /[\w$]+/y;

export { languageName } from './language';

/** `more` adds languages (see highlight-all.ts); the core set costs about 1 KB gzip less than all of them, which matters on a page that only shows a code sample. */
export function tokenize(code: string, lang: string = text, more?: Grammars): Run[] {
  const rules = grammars[lang] ?? more?.[lang];
  if (!rules) return [{ text: code }];
  const runs: Run[] = [];
  let plain = '';
  const flush = () => {
    if (plain) runs.push({ text: plain });
    plain = '';
  };
  for (let i = 0; i < code.length; ) {
    let hit: [Role, string] | null = null;
    for (const [role, re] of rules) {
      re.lastIndex = i;
      const m = re.exec(code);
      if (m && m[0].length) {
        hit = [role, m[0]];
        break;
      }
    }
    if (hit) {
      flush();
      runs.push({ role: hit[0], text: hit[1] });
      i += hit[1].length;
    } else {
      // No rule starts here: take the whole word, so no rule can match from the middle of an identifier.
      word.lastIndex = i;
      const w = word.exec(code)?.[0] || code[i]!;
      plain += w;
      i += w.length;
    }
  }
  flush();
  return runs;
}
