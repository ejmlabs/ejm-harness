import { useMemo } from 'react';
import { CodeSource } from './CodeBlockFrame';
import { languageName, tokenize } from './highlight-all';

/** Highlighted source only. Compile this on the server when its figure and controls hydrate separately. */
export function CodeHighlight({ code, lang = 'text', label }: { code: string; lang?: string; label?: string }) {
  const body = code.replace(/\n$/, '');
  const runs = useMemo(() => tokenize(body, lang), [body, lang]);
  return <CodeSource code={body} label={label ?? languageName(lang)}>
    {runs.map((run, i) => run.role ? <span key={i} data-r={run.role}>{run.text}</span> : run.text)}
  </CodeSource>;
}
