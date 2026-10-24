import { Link } from '@ejm/ssg';
import { Button, Input, Page, SectionHead, Table, Text } from '@ejm/ui';
import { useState } from 'react';

const columns = [{ key: 'player', label: 'Player' }, { key: 'games', label: 'Games' }, { key: 'points', label: 'Points' }];
const rows = [{ player: 'Alex', games: 12, points: 8 }, { player: 'Sam', games: 10, points: 6 }];
const states = ['default', 'loading', 'empty', 'error'] as const;
const densities = ['comfortable', 'compact'] as const;

/** A complete application composition. The data and saved form values are local examples. */
export default function ApplicationLayoutPage() {
  const [saved, setSaved] = useState('The preview has no saved values.');
  return (
    <Page variant="application" data-application-layout>
      <SectionHead purpose="application" as="h1" id="application-title" title="Application layout">
        <Text as="p" data-layout-description>
          A compact page with related fields, readable links and clear table states. These are simulated examples; the form saves only in this page preview.
        </Text>
      </SectionHead>

      <section aria-labelledby="application-form-title" className="grid gap-content">
        <h2 id="application-form-title" className="text-heading">Player preferences</h2>
        <Text as="p">
          Read the <Link href="/docs/components/" className="link" data-layout-inline>component guide</Link> for field and action contracts. Links within a sentence keep the same text flow.
        </Text>
        <form className="grid max-w-measure gap-content" data-layout-form onSubmit={event => {
          event.preventDefault();
          const values = new FormData(event.currentTarget);
          setSaved(`Saved for this preview: ${String(values.get('name'))}.`);
        }}>
          <div className="grid gap-field" data-layout-field>
            <label htmlFor="application-name" className="text-meta text-muted">Display name</label>
            <Input id="application-name" name="name" defaultValue="Alex" required aria-describedby="application-name-help" />
            <Text as="p" id="application-name-help" className="text-sm text-muted">Use a name other players can recognise.</Text>
          </div>
          <div className="grid gap-field" data-layout-field>
            <label htmlFor="application-email" className="text-meta text-muted">Recovery email, optional</label>
            <Input id="application-email" name="email" type="email" aria-describedby="application-email-help" />
            <Text as="p" id="application-email-help" className="text-sm text-muted">This example sends no email and stores no account.</Text>
          </div>
          <div className="grid justify-items-start gap-field">
            <Button type="submit" data-layout-action>Save preview</Button>
            <Text as="p" role="status" className="text-sm text-muted">{saved}</Text>
          </div>
        </form>
      </section>

      {densities.map(density => (
        <section key={density} aria-labelledby={`application-${density}-title`} className="grid gap-content">
          <h2 id={`application-${density}-title`} className="text-heading">{density === 'comfortable' ? 'Comfortable' : 'Compact'} tables</h2>
          {states.map(state => (
            <div key={state} className="grid gap-field" data-layout-table={state} data-layout-density={density}>
              <h3 className="text-meta text-muted">{state === 'default' ? 'Populated' : state[0]!.toUpperCase() + state.slice(1)}</h3>
              <Table columns={columns} rows={state === 'default' ? rows : []} state={state} density={density}
                label={`${density} ${state} example`}
                message={state === 'empty' ? 'No games have finished in this example.' : state === 'error' ? 'The example results could not load.' : undefined} />
            </div>
          ))}
        </section>
      ))}
    </Page>
  );
}
