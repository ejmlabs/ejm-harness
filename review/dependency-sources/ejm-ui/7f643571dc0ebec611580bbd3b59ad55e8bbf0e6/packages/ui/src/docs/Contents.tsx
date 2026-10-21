import { pickCurrentHeading } from '@ejm/core';
import { useEffect, useState, type MouseEvent } from 'react';
import { cn } from '../lib/utils';
import { prefersReducedMotion } from './hooks';

export interface ContentsItem {
  id: string;
  label: string;
  /** 2 for a section, 3 for a subsection (indented and muted). */
  level: 2 | 3;
}

/**
 * "On this page". Every entry is a plain link to a heading id, so it jumps with no script. With script, the heading being
 * read is marked with aria-current (the last heading above the top quarter of the window, and the last entry at the end
 * of the page), and activating an entry moves focus to the heading and scrolls to it, smoothly unless the visitor
 * prefers reduced motion. Modified clicks keep their normal meaning.
 */
export function Contents({ items, label = 'On this page', className }: { items: readonly ContentsItem[]; label?: string; className?: string }) {
  const [current, setCurrent] = useState<string | null>(null);
  const ids = items.map((i) => i.id).join('\n');

  useEffect(() => {
    const headings = ids.split('\n').map((id) => document.getElementById(id)).filter((h): h is HTMLElement => !!h);
    if (!headings.length) return;
    const update = () => {
      const root = document.documentElement;
      const offsets = headings.map((h) => ({ id: h.id, top: h.getBoundingClientRect().top + window.scrollY }));
      const found = pickCurrentHeading(
        offsets,
        { scrollTop: window.scrollY, viewportHeight: window.innerHeight, scrollHeight: root.scrollHeight },
        window.innerHeight * 0.25,
      );
      setCurrent(found ?? headings[0]!.id);
    };
    // A heading crossing the line near the top of the window changes the current entry. The very end of the page may never
    // bring the last heading to that line, so scroll also checks (once per frame).
    const io = typeof IntersectionObserver === 'function' ? new IntersectionObserver(update, { rootMargin: '0px 0px -75% 0px' }) : null;
    headings.forEach((h) => io?.observe(h));
    let queued = false;
    const onScroll = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        update();
      });
    };
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    update();
    return () => {
      io?.disconnect();
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, [ids]);

  const go = (event: MouseEvent<HTMLAnchorElement>, id: string) => {
    if (event.button || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const heading = document.getElementById(id);
    if (!heading) return;
    event.preventDefault();
    // On a phone the list sits in a native <details>: fold it away once the reader has chosen.
    const details = event.currentTarget.closest('details');
    const summary = details?.querySelector('summary');
    if (details && summary && getComputedStyle(summary).display !== 'none') details.open = false;
    if (!heading.hasAttribute('tabindex')) heading.tabIndex = -1;
    heading.focus({ preventScroll: true });
    heading.scrollIntoView({ behavior: prefersReducedMotion() ? 'auto' : 'smooth', block: 'start' });
    history.pushState(null, '', `#${id}`);
    setCurrent(id);
  };

  return (
    <nav aria-label={label} className={className}>
      {/* Below xl the list sits in a disclosure whose button already says this. */}
      <p className="hidden text-meta text-muted xl:block" aria-hidden="true">
        {label}
      </p>
      <ul className="xl:mt-3">
        {items.map((item) => (
          <li key={item.id}>
            <a
              href={`#${item.id}`}
              onClick={(e) => go(e, item.id)}
              aria-current={current === item.id ? 'true' : undefined}
              className={cn(
                'touch-target focus-ring block border-s py-1.5 text-[0.9375rem] leading-snug no-underline transition-colors duration-fast',
                item.level === 3 ? 'ps-7' : 'ps-4',
                'border-line text-muted hover:border-control hover:text-fg aria-[current=true]:border-fg aria-[current=true]:text-fg',
              )}
            >
              {item.label}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
