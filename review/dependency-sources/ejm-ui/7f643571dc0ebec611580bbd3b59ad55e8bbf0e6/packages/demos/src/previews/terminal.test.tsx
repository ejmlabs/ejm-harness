// @vitest-environment node
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import * as interactive from '../views/terminal';
import * as recorded from './terminal';
import { cliFrame } from './frames/cli';
import timeline from './recordings/cli.json';
import type { CliBlock } from '../cli';

it('keeps the recorded terminal presentation identical to the public color view', () => {
  const blocks = timeline.beats.flatMap(beat => beat.blocks ?? []) as CliBlock[];
  const header = { provider: cliFrame.provider, title: cliFrame.title, write: cliFrame.write, cwd: cliFrame.cwd, sessionId: cliFrame.sessionId };
  const actual = <><recorded.CliHeader {...header} />{blocks.map((block, index) => <recorded.TermBlock key={index} block={block} />)}<recorded.CliStatus {...cliFrame} /><recorded.CliHints hints={cliFrame.hints} /></>;
  const expected = <><interactive.CliHeader {...header} />{blocks.map((block, index) => <interactive.TermBlock key={index} block={block} />)}<interactive.CliStatus {...cliFrame} /><interactive.CliHints hints={cliFrame.hints} /></>;
  expect(renderToStaticMarkup(actual)).toBe(renderToStaticMarkup(expected));
});
