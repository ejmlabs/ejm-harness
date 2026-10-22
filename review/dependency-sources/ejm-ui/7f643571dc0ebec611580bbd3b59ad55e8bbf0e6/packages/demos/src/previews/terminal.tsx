import type { CliBlock, CliLine } from '../cli';
import type { Role } from '../text';
import { cx } from './cx';

const roleClass: Record<NonNullable<Role>, string> = {
  accent: 'text-ui-accent',
  dim: 'text-ui-dim',
  ok: 'text-ui-success',
  err: 'text-ui-error',
  special: 'text-ui-special',
};

/** Fixed colors from the bundled recording. Public interactive views also support plain mode. */
export function roleTone(r: Role, bold?: boolean): string {
  return cx(r && roleClass[r], bold && 'font-bold');
}

/** Code-like lines (tool output, diffs) are cut with an ellipsis; prose wraps. */
const isCode = (b: CliBlock, l: CliLine) => b.kind === 'tool' || (b.kind === 'assistant' && l.r !== null && l.r !== 'accent');

function blockTitle(block: CliBlock) {
  switch (block.kind) {
    case 'you':
      return <div className={roleTone('accent')}>you</div>;
    case 'assistant':
      return <div className="font-bold text-ui-text-strong">ejm-cli</div>;
    case 'tool':
      return <div className="overflow-hidden text-ellipsis whitespace-pre text-ui-dim">{'▸ ' + block.title}</div>;
    case 'notice':
      return <div className="text-ui-warning">{'! ' + block.title}</div>;
    default:
      return <div className="text-ui-error">{'✗ ' + block.title}</div>;
  }
}

export function TermBlock({ block }: { block: CliBlock }) {
  const dflt: Role = block.kind === 'tool' ? 'dim' : block.kind === 'error' ? 'err' : null;
  return (
    <div data-kind={block.kind} className="mb-[0.7em]">
      {/* A titleless assistant block (the compact frame's diff) still names who speaks, as the real transcript does. */}
      {blockTitle(block)}
      {block.lines.map((l, i) => (
        <div
          key={i}
          className={cx(roleTone(l.r ?? dflt, l.bold), isCode(block, l) ? 'overflow-hidden text-ellipsis whitespace-pre' : 'whitespace-pre-wrap break-words')}
        >
          {l.t || ' '}
        </div>
      ))}
    </div>
  );
}

const oneLine = 'overflow-hidden whitespace-pre text-ellipsis';

export function CliHeader({ provider, title, write, shell = false, cwd, sessionId }: { provider: string; title: string; write: boolean; shell?: boolean; cwd: string; sessionId: string }) {
  return (
    <div className="shrink-0">
      <div className={oneLine}>
        <span className="text-ui-accent font-bold">{'>_ ejm-cli '}</span>
        {provider}
        <span className="text-ui-dim">{'  ·  ' + title}</span>
      </div>
      <div className={oneLine}>
        <span className={write ? 'text-ui-warning' : 'text-ui-dim'}>{'grants: read' + (write ? ' + write' : '') + (shell ? ' + shell (current user)' : '')}</span>
        <span className="text-ui-dim">{'  ·  ' + cwd + '  ·  ' + sessionId}</span>
      </div>
    </div>
  );
}

/** Counts and state on one line. Parts that do not fit wrap onto a hidden second line, so they drop from the end. */
export function CliStatus({ agents, models, tools }: { agents: number; models: number; tools: number }) {
  return (
    <div className="flex max-h-[1lh] flex-wrap overflow-hidden whitespace-pre text-ui-dim">
      <span>{' ' + agents + ' agents'}</span>
      <span>{' · models ' + models + '+0q'}</span>
      <span>{' · tools ' + tools + '+0q'}</span>
      <span> · Ready</span>
    </div>
  );
}

export function CliHints({ hints }: { hints: [string, string][] }) {
  return (
    <ul aria-label="Keyboard shortcuts" className="m-0 flex max-h-[1lh] list-none flex-wrap overflow-hidden whitespace-pre p-0 text-ui-dim">
      {hints.map(([k, a], i) => (
        <li key={k}>{(i ? ' · ' : ' ') + k + ' ' + a}</li>
      ))}
    </ul>
  );
}
