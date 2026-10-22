import { lazy, Suspense, useLayoutEffect, useState, type ComponentType } from 'react';

type Page = { default: ComponentType };

function ReadyPage({ Body }: { Body: ComponentType }) {
  const [ready, setReady] = useState(false);
  // This commits only after the body has loaded and its handlers are attached.
  useLayoutEffect(() => setReady(true), []);
  return <fieldset disabled={!ready} className="min-w-0">
    <legend className="sr-only">Page examples</legend>
    {!ready && <p role="status" className="page py-2 text-sm text-muted">Page controls are loading. The examples remain readable.</p>}
    <Body />
  </fieldset>;
}

/** Keep complete server HTML while page code loads immediately after the small site entry. */
export async function island(load: () => Promise<Page>, title: string): Promise<Page> {
  const Body = import.meta.env.SSR ? (await load()).default : lazy(load);
  return {
    default: () => (
      <Suspense fallback={<div className="page py-rhythm"><h1 className="text-display">{title}</h1><p role="status">Loading page controls…</p></div>}>
        <ReadyPage Body={Body} />
      </Suspense>
    ),
  };
}
