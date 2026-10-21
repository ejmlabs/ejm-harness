import { createTabsState, tabIndexFor, tabsKeyAction, tabsReducer } from '@ejm/core';
import { useEffect, useId, useReducer, useRef, useSyncExternalStore, type KeyboardEvent, type ReactNode } from 'react';
import { cn } from '../lib/utils';
import { tabPreference, useHydrated } from './hooks';

export interface CodeTabContent {
  /** The tab name: CSS, TypeScript, Rust. Groups follow each other when they share a name. */
  label: string;
  children?: ReactNode;
}

/**
 * The same task in several languages. The WAI-ARIA tabs pattern with automatic activation: Left and Right move and
 * wrap, Home and End jump, and only the selected tab is in the tab order. Choosing a language also selects it in every
 * other group that has it.
 *
 * Without scripts, and on the server, every panel is shown stacked under its own label. After hydration the labels
 * give way to a tab list and only the selected panel shows.
 */
export function CodeTabsFrame({ label = 'Language', tabs, className }: { label?: string; tabs: readonly CodeTabContent[]; className?: string }) {
  const hydrated = useHydrated();
  const uid = useId();
  const [state, dispatch] = useReducer(tabsReducer, tabs.length, (count) => createTabsState({ count }));
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const wantFocus = useRef(false);
  const pref = useSyncExternalStore(tabPreference.subscribe, tabPreference.get, () => null);
  const names = tabs.map((t) => t.label).join('\n');

  // Another group chose a language this one has: follow it.
  useEffect(() => {
    const i = pref ? names.split('\n').indexOf(pref) : -1;
    if (i >= 0) dispatch({ type: 'select', index: i });
  }, [pref, names]);

  // A key moved focus: put the DOM focus on the new tab after it renders.
  useEffect(() => {
    if (wantFocus.current) {
      wantFocus.current = false;
      buttons.current[state.focused]?.focus();
    }
  });

  const remember = (index: number) => {
    const tab = tabs[index];
    if (tab) tabPreference.set(tab.label);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const action = tabsKeyAction(event.key);
    if (!action || action.type === 'activate') return;
    event.preventDefault();
    wantFocus.current = true;
    dispatch(action);
    remember(tabsReducer(state, action).selected);
  };

  return (
    <div className={cn('my-8', className)}>
      {hydrated && (
        <div role="tablist" aria-label={label} className="hairline flex gap-6 overflow-x-auto" onKeyDown={onKeyDown}>
          {tabs.map((t, i) => (
            <button
              key={t.label}
              ref={(el) => {
                buttons.current[i] = el;
              }}
              type="button"
              role="tab"
              id={`${uid}-tab-${i}`}
              aria-selected={i === state.selected}
              aria-controls={`${uid}-panel-${i}`}
              tabIndex={tabIndexFor(state, i)}
              onClick={() => {
                dispatch({ type: 'select', index: i });
                remember(i);
              }}
              className={cn(
                'touch-target focus-ring -mb-px inline-flex min-h-11 items-center whitespace-nowrap border-b-2 text-meta transition-colors duration-fast',
                i === state.selected ? 'border-fg text-fg' : 'border-transparent text-muted hover:text-fg',
              )}
            >
              {t.label}
            </button>
          ))}
        </div>
      )}
      {tabs.map((t, i) => (
        <div
          key={t.label}
          id={`${uid}-panel-${i}`}
          role={hydrated ? 'tabpanel' : undefined}
          aria-labelledby={hydrated ? `${uid}-tab-${i}` : undefined}
          hidden={hydrated && i !== state.selected}
          className={hydrated ? undefined : 'mt-6 first:mt-0'}
        >
          {!hydrated && <p className="text-meta text-muted">{t.label}</p>}
          {t.children}
        </div>
      ))}
    </div>
  );
}
