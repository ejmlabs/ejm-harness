import { tokenize, type Role, type Run } from '@ejm/ui/docs/highlight';
import { cx } from './cx';

const synClass: Record<Role, string> = {
  k: 'text-syn-keyword',
  s: 'text-syn-string',
  c: 'text-syn-comment',
  n: 'text-syn-number',
  t: 'text-syn-type',
  f: 'text-syn-function',
  m: 'text-syn-macro',
  p: 'text-syn-property',
  v: 'text-syn-constant',
};

/** Highlighted text from the docs tokenizer. Joining the spans gives back the input exactly. */
export function Syntax({ code, lang }: { code: string; lang: string }) {
  return (
    <>
      {tokenize(code, lang).map((r, i) => (r.role ? <span key={i} className={synClass[r.role]}>{r.text}</span> : r.text))}
    </>
  );
}

/** Splits highlighted runs at newlines, so a multi-line comment or string still colors every line of it. */
export function tokenLines(code: string, lang: string): Run[][] {
  const lines: Run[][] = [[]];
  for (const r of tokenize(code, lang)) {
    r.text.split('\n').forEach((part, i) => {
      if (i > 0) lines.push([]);
      if (part) lines.at(-1)!.push(r.role ? { role: r.role, text: part } : { text: part });
    });
  }
  if (lines.at(-1)!.length === 0) lines.pop();
  return lines;
}

/** Numbered, highlighted code. `flash` lines fade in once (an opacity pulse that does nothing under reduced motion). `changed` holds zero-based lines to mark: the gutter shows "+" as well as a tone, so the mark never relies on color. */
export function CodeLines({ code, lang, changed = [], flash = [], className }: { code: string; lang: string; changed?: number[]; flash?: number[]; className?: string }) {
  return (
    <div className={cx('min-w-0', className)}>
      {tokenLines(code, lang).map((runs, i) => {
        const on = changed.includes(i);
        return (
          <div key={i} data-line={i + 1} data-changed={on || undefined} className={cx('flex whitespace-pre', on && 'bg-ui-diff-add-bg', flash.includes(i) && 'animate-fade')}>
            <span aria-hidden className="w-[3.5ch] shrink-0 select-none pe-[1ch] text-end text-ui-dim">{i + 1}</span>
            <span aria-hidden className={cx('w-[1.5ch] shrink-0 select-none', on && 'text-ui-diff-add')}>{on ? '+' : ''}</span>
            <span className="min-w-0 overflow-hidden text-ellipsis">
              {runs.map((r, j) => (r.role ? <span key={j} className={synClass[r.role]}>{r.text}</span> : r.text))}
              {runs.length === 0 && ' '}
            </span>
          </div>
        );
      })}
    </div>
  );
}
