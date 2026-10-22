import { Moon, Sun } from 'lucide-react';
import { useState, type ComponentPropsWithoutRef } from 'react';
import { useTheme } from '../hooks/useTheme';
import { cn } from '../lib/utils';

/**
 * Switches between light and dark and remembers the choice (see useTheme). The button is named for what it does
 * ("Switch to dark theme"), shows the icon of the theme it will switch to, and announces the result politely.
 * The icons are chosen with the `dark:` variant, so the right one shows before hydration with no flash.
 * An explicit switch crossfades with the native View Transitions API when available and motion is allowed.
 * Without scripts it does nothing; the page still follows the system theme.
 */
export function ThemeToggle({ className, ...rest }: Omit<ComponentPropsWithoutRef<'button'>, 'onClick' | 'type'>) {
  const { resolved, toggle } = useTheme();
  const [announce, setAnnounce] = useState('');
  const next = resolved === 'dark' ? 'light' : 'dark';
  return (
    <>
      <button
        type="button"
        aria-label={`Switch to ${next} theme`}
        onClick={() => {
          let changed = false;
          const apply = () => {
            if (changed) return;
            changed = true;
            toggle();
            setAnnounce(`${next[0]!.toUpperCase()}${next.slice(1)} theme on`);
          };
          if (!document.startViewTransition || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
            apply();
            return;
          }
          try {
            const transition = document.startViewTransition(apply);
            // A hidden or busy tab must still save and show the requested theme.
            const pending = setTimeout(() => {
              if (!changed) {
                transition.skipTransition();
                apply();
              }
            }, 300);
            void transition.ready.catch(apply);
            void transition.updateCallbackDone.catch(apply);
            void transition.finished.catch(() => {}).finally(() => clearTimeout(pending));
          } catch {
            apply();
          }
        }}
        className={cn(
          'inline-flex size-11 shrink-0 items-center justify-center rounded-pill text-fg transition-colors duration-base ease-standard hover:bg-tone active:bg-line',
          className,
        )}
        {...rest}
      >
        <Moon aria-hidden className="size-5 dark:hidden" />
        <Sun aria-hidden className="hidden size-5 dark:block" />
      </button>
      <span role="status" className="sr-only">
        {announce}
      </span>
    </>
  );
}
