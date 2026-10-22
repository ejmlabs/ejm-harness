import { useEffect, useRef, type HTMLAttributes, type ReactNode, type Ref } from 'react';
import { Button } from '../components/ui/button';

export interface GameWorkspaceProps extends Omit<HTMLAttributes<HTMLElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'> {
  board: ReactNode;
  children: ReactNode;
  after?: ReactNode;
  label?: string;
  detailsLabel?: string;
  ref?: Ref<HTMLElement>;
}

/** The board retains room to play while related state moves below it on a phone. */
export function GameWorkspace({ board, children, after, label, detailsLabel, ref, ...props }: GameWorkspaceProps) {
  return <section {...props} ref={ref} aria-label={label ?? props['aria-label']} className="focus-ring grid min-w-0 grid-cols-1 items-start gap-8 lg:grid-cols-[minmax(0,min(42rem,max(16rem,calc(100svh_-_23rem))))_minmax(16rem,1fr)] lg:gap-12">
    <div className="min-w-0">{board}</div>
    <aside aria-label={detailsLabel} className="grid min-w-0 grid-cols-1 gap-6">{children}</aside>
    {after && <div className="min-w-0 lg:col-span-2">{after}</div>}
  </section>;
}

export function PlayerStrip({ name, detail, children }: { name: ReactNode; detail?: ReactNode; children?: ReactNode }) {
  return <div className="flex min-w-0 items-center justify-between gap-4">
    <div className="grid min-w-0 gap-1 [overflow-wrap:anywhere]">
      <strong className="text-body font-semibold">{name}</strong>
      {detail && <span className="text-meta text-muted">{detail}</span>}
    </div>
    {children && <div className="shrink-0">{children}</div>}
  </div>;
}

export function BoardPlaceholder({ children }: { children?: ReactNode }) {
  return <div className="grid aspect-square min-w-0 items-center justify-items-center bg-surface p-6">{children}</div>;
}

export function MovePairs({ moves, label = 'Moves' }: { moves: readonly { number: number; white?: string; black?: string }[]; label?: string }) {
  return <ol role="list" aria-label={label} className="grid max-h-80 min-w-0 gap-2 overflow-y-auto overscroll-contain font-mono text-meta tabular-nums">
    {moves.map(move => <li key={move.number} className="grid grid-cols-[3ch_1fr_1fr] gap-3">
      <span className="text-muted">{move.number}.</span><span>{move.white ?? ''}</span><span>{move.black ?? ''}</span>
    </li>)}
  </ol>;
}

export interface MoveHistoryProps {
  items: readonly { id: number; number: number; color: 'w' | 'b'; san: string }[];
  selectedId: number;
  onSelect: (id: number) => void;
  label?: string;
}

/** Keep the selected move visible by scrolling only this bounded list, never its document. */
export function MoveHistory({ items, selectedId, onSelect, label = 'Moves' }: MoveHistoryProps) {
  const listRef = useRef<HTMLOListElement>(null);
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const selected = list.querySelector('[aria-current="step"]')?.closest('li');
    if (!selected) { list.scrollTop = 0; return; }
    const top = selected.offsetTop;
    const bottom = top + selected.offsetHeight;
    if (top < list.scrollTop) list.scrollTop = top;
    else if (bottom > list.scrollTop + list.clientHeight) list.scrollTop = bottom - list.clientHeight;
  }, [selectedId, items.length]);
  return <ol ref={listRef} role="list" aria-label={label} className="relative grid max-h-80 min-w-0 gap-1 overflow-y-auto overscroll-contain font-mono text-meta tabular-nums">
    {items.map(item => <li key={item.id}>
      <Button variant={item.id === selectedId ? 'secondary' : 'ghost'} density="compact" aria-current={item.id === selectedId ? 'step' : undefined} onClick={() => onSelect(item.id)} className="w-full justify-start">
        {item.number}{item.color === 'w' ? '.' : '…'} {item.san}
      </Button>
    </li>)}
  </ol>;
}
