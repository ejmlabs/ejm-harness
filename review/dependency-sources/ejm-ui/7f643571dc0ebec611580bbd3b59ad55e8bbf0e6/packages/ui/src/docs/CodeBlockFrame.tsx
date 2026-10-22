import { copyAnnouncement, copyLabel, createCopy, type CopyState } from '@ejm/core';
import { useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { cn } from '../lib/utils';
import { useHydrated } from './hooks';
import { languageName } from './language';

export interface CodeBlockFrameProps {
  code: string;
  /** Grammar and label: ts, tsx, js, css, html, json, rust, sh. Anything else shows as plain text under its own name. */
  lang?: string;
  /** Text for the label when it should differ from the language name, for example a file name. */
  label?: string;
  /** Turn the copy control off for output that is not meant to be run. */
  copyable?: boolean;
  /** Hide the language label from sight (it stays for assistive technology), for a block under a tab that already names it. */
  quietLabel?: boolean;
  className?: string;
  children?: ReactNode;
}

function useCopyControl(getText: () => string, onFail: () => void) {
  const [copy] = useState(() =>
    createCopy({
      write: async (text) => {
        if (!navigator.clipboard?.writeText) throw new Error('clipboard unavailable');
        await navigator.clipboard.writeText(text);
      },
    }),
  );
  const state = useSyncExternalStore<CopyState>(copy.machine.subscribe, copy.machine.getState, () => 'idle');
  return {
    state,
    run: async () => {
      await copy.copy(getText());
      if (copy.machine.getState() === 'failed') onFail();
    },
  };
}

/**
 * Code the reader will copy. A label row (language on the left, Copy on the right) over a scrolling code area on the
 * flat `ui-editor` ground, framed by two hairlines. The code area is a labelled, focusable region, so a keyboard user
 * can scroll it on a narrow screen. The Copy button appears once scripts run; without them the code is still selectable.
 */
export function CodeBlockFrame({ code, lang = 'text', label, copyable = true, quietLabel = false, className, children }: CodeBlockFrameProps) {
  const name = label ?? languageName(lang);
  const body = code.replace(/\n$/, '');
  const hydrated = useHydrated();
  const figure = useRef<HTMLElement>(null);
  const { state, run } = useCopyControl(
    () => figure.current?.querySelector('pre')?.textContent ?? body,
    () => {
      // Clipboard blocked: select the code so the keyboard shortcut works.
      const el = figure.current?.querySelector('pre');
      if (!el) return;
      const range = document.createRange();
      range.selectNodeContents(el);
      const sel = getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    },
  );
  return (
    <figure ref={figure} className={cn('my-8 scheme-page bg-ui-editor text-ui-text border-y border-line', className)}>
      <figcaption className="flex items-center justify-between gap-4 ps-4 pe-2 text-meta text-ui-text-muted">
        <span className={quietLabel ? 'sr-only' : undefined}>{name}</span>
        {hydrated && copyable && (
          <button
            type="button"
            onClick={run}
            aria-label={state === 'idle' ? `Copy ${name} code` : undefined}
            className="touch-target focus-ring ms-auto inline-flex min-h-11 items-center px-3 text-ui-text-muted transition-colors duration-fast hover:text-ui-text-strong"
          >
            {copyLabel(state)}
          </button>
        )}
      </figcaption>
      {children ?? <CodeSource code={body} label={name} />}
      <span role="status" className="sr-only">
        {hydrated ? copyAnnouncement(state) : ''}
      </span>
    </figure>
  );
}

// The roles share the gated syntax palette. Each is a gated `syn-*` window color; the code ground is `ui-editor`, the ground they are gated on.
const roles =
  '[&_[data-r=k]]:text-syn-keyword [&_[data-r=s]]:text-syn-string [&_[data-r=c]]:text-syn-comment [&_[data-r=n]]:text-syn-number [&_[data-r=t]]:text-syn-type [&_[data-r=f]]:text-syn-function [&_[data-r=m]]:text-syn-macro [&_[data-r=p]]:text-syn-property [&_[data-r=v]]:text-syn-constant';

/** The labelled source region, with plain code or already-tokenized children. */
export function CodeSource({ code, label, children }: { code: string; label: string; children?: ReactNode }) {
  return <pre tabIndex={0} role="region" aria-label={`${label} code`}
    className={cn('focus-ring overflow-x-auto px-4 pb-4 pt-1 font-mono text-[0.8125rem] leading-6 sm:text-sm sm:leading-6', roles)}>
    <code>{children ?? code}</code>
  </pre>;
}
