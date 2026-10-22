import { useState } from 'react';
import { Link } from '@ejm/ssg';
import { PROJECT_LIST } from '@ejm/site/projects';
import {
  AppActions, AppField, AppForm, AppSection, Band, Button, Fact, Facts, Hero, Input,
  MetricGrid, Row, Rows, SectionBody, SectionHead, Stat, Text,
} from '@ejm/ui';
import { pages } from '../pages';

function ComponentExample() {
  const [saved, setSaved] = useState('Changes stay on this page.');
  return <AppSection aria-labelledby="example-title">
    <SectionHead purpose="application" kicker="Simulated component example" title="Project settings" id="example-title" />
    <AppForm onSubmit={event => {
      event.preventDefault();
      const name = String(new FormData(event.currentTarget).get('name'));
      setSaved(`Saved ${name} for this preview.`);
    }}>
      <AppField>
        <label htmlFor="example-name">Project name</label>
        <Input id="example-name" name="name" defaultValue="Harbor" required />
      </AppField>
      <AppActions><Button type="submit">Save preview</Button></AppActions>
      <Text as="p" variant="muted" role="status">{saved}</Text>
    </AppForm>
  </AppSection>;
}

export default function Home() {
  return (
    <>
      <Hero
        title="Build with ejm-ui."
        aside={<ComponentExample />}
        actions={
          <>
            <Button asChild arrow><Link href="/docs/">Read the docs</Link></Button>
            <Button asChild variant="link"><Link href="/components/">See the components</Link></Button>
          </>
        }
      >
        <p>Components, tokens and layouts for ejmlabs sites and apps.</p>
      </Hero>

      <Band id="pages" tone="sage" aria-labelledby="pages-title">
        <SectionHead kicker="01 / Pages" title="Explore the system." id="pages-title" />
        <SectionBody>
          <Rows aria-label="Gallery pages">
            {pages.map(p => <Row key={p.path} title={p.label} description={p.description} meta={p.meta} href={p.path} />)}
          </Rows>
        </SectionBody>
      </Band>

      <Band tone="paper" aria-labelledby="surfaces-title">
        <SectionHead kicker="02 / Surfaces" title="In use." id="surfaces-title" />
        <SectionBody>
          <Rows aria-label="ejmlabs surfaces">
            {PROJECT_LIST.filter(p => p.id !== 'ejmlabs').map(p => (
              <Row key={p.id} title={p.name} description={p.tagline} meta={p.host} href={p.url} external />
            ))}
          </Rows>
        </SectionBody>
      </Band>

      <Band tone="mist" aria-labelledby="system-title">
        <SectionHead kicker="03 / System" title="Tokens and budgets." id="system-title" />
        <SectionBody>
          <Facts>
            <Fact label="fonts">Manrope and IBM Plex Mono</Fact>
            <Fact label="ground">Paper, Mist, Sage and Ink</Fact>
            <Fact label="groups">Type, space, hairlines and bands</Fact>
            <Fact label="runtime">React 19, TypeScript, Tailwind CSS 4</Fact>
          </Facts>
        </SectionBody>
        <SectionBody>
          <MetricGrid>
            <Stat figure="4" label="bands" />
            <Stat figure="2" label="themes" />
            <Stat figure="95" label="KB gzip script budget" />
            <Stat figure="14" label="KB gzip CSS budget" />
          </MetricGrid>
        </SectionBody>
      </Band>

      <Band tone="ink" aria-labelledby="checks-title">
        <SectionHead kicker="04 / Checks" title="Check your build." id="checks-title">
          <Button asChild arrow><Link href="/docs/checks/">Checks and budgets</Link></Button>
        </SectionHead>
      </Band>
    </>
  );
}
