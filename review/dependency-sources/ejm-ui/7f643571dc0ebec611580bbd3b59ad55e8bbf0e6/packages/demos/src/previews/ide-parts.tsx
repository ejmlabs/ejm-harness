import type { ReactNode } from 'react';
import type { ChatItem, DiffLine, ToolItem } from '../ide';
import { Syntax } from './code';
import { cx } from './cx';

const oneLine = 'overflow-hidden text-ellipsis whitespace-pre';

const diffTone: Record<DiffLine['kind'], string> = {
  add: 'bg-ui-diff-add-bg text-ui-diff-add',
  del: 'bg-ui-diff-del-bg text-ui-diff-del',
  hunk: 'text-ui-special',
  file: 'text-ui-dim',
  ctx: 'text-ui-text-muted',
};

/** A diff with a +, - or space in the margin, so the change reads without color. The code after the marker is highlighted. */
export function Diff({ lines, lang = 'rust' }: { lines: DiffLine[]; lang?: string }) {
  return (
    <div className="my-[0.3em] overflow-hidden">
      {lines.map((l, i) => {
        const code = l.kind === 'hunk' || l.kind === 'file';
        return (
          <div key={i} className={cx('flex whitespace-pre', diffTone[l.kind])}>
            <span aria-hidden className="w-[2ch] shrink-0 select-none ps-[0.5ch]">{code ? '' : l.text[0]}</span>
            <span className="min-w-0 overflow-hidden text-ellipsis">{code ? l.text : l.kind === 'ctx' ? l.text.slice(1) : <Syntax code={l.text.slice(1)} lang={lang} />}</span>
            <span className="sr-inline">{l.kind === 'add' ? 'added: ' : l.kind === 'del' ? 'removed: ' : ''}</span>
          </div>
        );
      })}
    </div>
  );
}

const glyph: Record<NonNullable<ToolItem['state']> | 'none', string> = { ok: '✓', error: '×', running: '…', none: '→' };
const stateTone: Record<NonNullable<ToolItem['state']> | 'none', string> = { ok: 'text-ui-success', error: 'text-ui-error', running: 'text-ui-info', none: 'text-ui-warning' };

export function ToolCall({ item, children, passive = false }: { item: ToolItem; children?: ReactNode; passive?: boolean }) {
  const k = item.state ?? 'none';
  const open = (item as typeof item & { expanded?: boolean }).expanded ?? Boolean(item.diff);
  const header = <><span aria-hidden className={stateTone[k]}>{glyph[k]}</span><span className="edi-tool-name">{item.name.replaceAll('_', ' ')}</span><span className="edi-tool-arg">{item.arg}</span><span className="edi-tool-word">{item.detail ? item.detail + ' · ' : ''}{item.word === 'ok' ? 'completed' : item.word === 'applied' ? 'Applied' : item.word}</span></>;
  const body = <>
    {item.output && <div className="edi-tool-output">{item.output.map((line, index) => <div key={index} className={oneLine}>{line}</div>)}</div>}
    {item.diff && <div className="edi-tool-diff"><Diff lines={item.diff} /></div>}
    {item.note && <p className="text-ui-warning">{item.note}</p>}{children}
  </>;
  // A native summary remains an expandable control in accessibility trees even
  // with tabIndex=-1. The playback view needs readable content without that action.
  if (passive) return <div className="edi-tool"><div className="edi-tool-summary">{header}</div><div hidden={!open}>{body}</div></div>;
  return <details open={open} className="edi-tool"><summary className="edi-tool-summary">{header}</summary>{body}</details>;
}

export function ChatLine({ item, agent, passive = false }: { item: ChatItem; agent: string; passive?: boolean }) {
  switch (item.kind) {
    case 'user':
    case 'agent':
      return <article className="edi-message"><div className="edi-author">{item.kind === 'user' ? <span className="edi-you" /> : <span aria-hidden>⌘</span>}<strong>{item.kind === 'user' ? 'You' : agent}</strong></div><p>{item.text}</p></article>;
    case 'tool': return <ToolCall item={item} passive={passive} />;
    case 'caption': return <p className="text-ui-warning">{item.text}</p>;
    default: return <p className="text-ui-dim">Stopped.</p>;
  }
}
