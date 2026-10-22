import type { ReactNode } from 'react';

/** A Markdown table with all columns intact and a labelled local scroll region. */
export function DocTable({ caption, heads, rows }: { caption: string; heads: readonly ReactNode[]; rows: readonly (readonly ReactNode[])[] }) {
  return (
    <div role="region" aria-label={caption} tabIndex={0} className="focus-ring relative my-8 overflow-x-auto">
      <table className="w-full border-collapse text-start text-[0.9375rem] leading-snug">
        <caption className="sr-only">{caption}</caption>
        <thead><tr className="hairline text-meta text-muted">
          {heads.map((head, i) => <th key={i} scope="col" className="py-3 pe-6 text-start font-medium last:pe-0">{head}</th>)}
        </tr></thead>
        <tbody>{rows.map((row, i) => <tr key={i} className="hairline align-baseline">
          {heads.map((_, j) => j === 0
            ? <th key={j} scope="row" className="py-4 pe-6 text-start align-baseline font-medium text-fg last:pe-0">{row[j]}</th>
            : <td key={j} className="py-4 pe-6 align-baseline text-fg last:pe-0">{row[j]}</td>)}
        </tr>)}</tbody>
      </table>
    </div>
  );
}
