// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { createManualClock } from './core';
import { createIdeDemo, type IdeState, type ToolItem } from './ide';
import { ideScenario, project } from './scenarios';

const make = (extra: Parameters<typeof createIdeDemo>[0] = {}) => {
  const clock = createManualClock();
  const demo = createIdeDemo({ clock, ...extra });
  const prompt = (text: string) => demo.dispatch({ type: 'prompt', text });
  return { clock, demo, prompt };
};
const tools = (s: IdeState) => s.chat.filter((c): c is ToolItem => c.kind === 'tool');
const agentTexts = (s: IdeState) => s.chat.filter((c) => c.kind === 'agent').map((c) => (c as { text: string }).text);
const retry = project.files.find((f) => f.path === 'src/retry.rs')!.text;

describe('first scripted session', () => {
  it('plays "Explain this repository" to the end', () => {
    const { demo, clock, prompt } = make();
    prompt(ideScenario.chips[0]!.prompt);
    expect(demo.state.running).toBe(true);
    clock.runAll();
    const s = demo.state;
    expect(s.running).toBe(false);
    expect(s.chatTitle).toBe('Explain this repository');
    expect(s.chat.map((c) => c.kind)).toEqual(['user', 'agent', 'tool', 'tool', 'agent']);
    expect(tools(s).map((t) => [t.name, t.arg, t.detail, t.state, t.word])).toEqual([
      ['workbench_read_file', 'README.md', '15 lines', 'ok', 'ok'],
      ['workbench_search_code', 'pub fn', '3 matches', 'ok', 'ok'],
    ]);
    expect(agentTexts(s)[1]).toMatch(/^harbor is a small order library\./);
    expect(s.live.at(-1)).toBe('Run finished.');
  });

  it('is deterministic for a seed', () => {
    const play = (seed?: number) => {
      const { demo, clock, prompt } = make({ seed });
      prompt('Explain this repository');
      clock.runAll();
      return { state: demo.state, now: clock.now };
    };
    expect(play()).toEqual(play());
    expect(play(1).now).not.toBe(play(5).now);
  });

  it('every chip finishes', () => {
    for (const c of ideScenario.chips) {
      const { demo, clock, prompt } = make({ reducedMotion: true });
      prompt(c.prompt);
      clock.runAll();
      expect(demo.state.running, c.id).toBe(false);
      expect(demo.state.live.at(-1), c.id).toBe('Run finished.');
    }
  });

  it('the search tool lists real matches from the project', () => {
    const { demo, clock, prompt } = make();
    prompt('Fix the flaky retry test');
    clock.runAll();
    const search = tools(demo.state).find((t) => t.name === 'workbench_search_code')!;
    expect(search.output).toEqual(['src/retry.rs:10: let now = Instant::now();', 'src/retry.rs:20: let started = Instant::now();']);
    expect(search.detail).toBe('2 matches');
  });

  it('a prompt that matches nothing gets the scripted fallback, badged as scripted', () => {
    const { demo, clock, prompt } = make();
    prompt('tell me a joke');
    clock.runAll();
    expect(demo.state.chat.at(-1)).toMatchObject({ kind: 'agent', text: ideScenario.fallback, scripted: true });
  });

  it('ignores an empty prompt and a prompt while a run is active', () => {
    const { demo, clock, prompt } = make();
    prompt('   ');
    expect(demo.state.chat).toEqual([]);
    prompt('Explain this repository');
    prompt('Fix the flaky retry test');
    clock.runAll();
    expect(demo.state.chat.filter((c) => c.kind === 'user')).toHaveLength(1);
  });
});

describe('review of a proposed write', () => {
  const proposed = () => {
    const x = make();
    x.prompt('Fix the flaky retry test');
    x.clock.runAll();
    return { ...x, write: () => tools(x.demo.state).find((t) => t.name === 'workbench_write_file')! };
  };

  it('proposes the diff and changes nothing until the visitor decides', () => {
    const { demo, write } = proposed();
    const t = write();
    expect(t).toMatchObject({ arg: 'src/retry.rs', state: null, word: 'proposed', review: true });
    expect(t.diff!.filter((d) => d.kind === 'del').map((d) => d.text)).toContain('-    let now = Instant::now();');
    expect(t.diff!.filter((d) => d.kind === 'add').length).toBeGreaterThan(0);
    expect(demo.state.files['src/retry.rs']).toBe(retry);
    expect(demo.state.changed).toEqual({});
    expect(demo.state.live).toContain('Review needed: Edi proposes a change to src/retry.rs. Accept or Reject.');
  });

  it('accept writes the file, marks the changed lines, flashes them and shows the file', () => {
    const { demo, write } = proposed();
    demo.dispatch({ type: 'closeFile', path: 'src/retry.rs' });
    expect(demo.state.active).toBeNull();
    demo.dispatch({ type: 'accept', id: write().id });
    const s = demo.state;
    expect(write()).toMatchObject({ state: 'ok', word: 'applied', review: false });
    expect(s.files['src/retry.rs']).toContain('pub fn window_open(started: Instant, now: Instant, budget: Duration) -> bool');
    expect(s.active).toBe('src/retry.rs');
    expect(s.openFiles).toEqual(['src/retry.rs']);
    const lines = s.files['src/retry.rs']!.split('\n');
    expect(s.changed['src/retry.rs']!.map((i) => lines[i])).toContain('pub fn window_open(started: Instant, now: Instant, budget: Duration) -> bool {');
    expect(s.flash).toEqual({ path: 'src/retry.rs', lines: s.changed['src/retry.rs'] });
    expect(agentTexts(s).at(-1)).toBe('Applied. window_open now takes the time as an argument.');
    // the flash is one-shot: opening another file and coming back does not repeat it
    demo.dispatch({ type: 'openFile', path: 'README.md' });
    demo.dispatch({ type: 'openFile', path: 'src/retry.rs' });
    expect(demo.state.flash).toBeNull();
    expect(demo.state.changed['src/retry.rs']).toEqual(s.changed['src/retry.rs']);
  });

  it('reject leaves the file alone and says so', () => {
    const { demo, write } = proposed();
    demo.dispatch({ type: 'reject', id: write().id });
    expect(write()).toMatchObject({ state: null, word: 'rejected', review: false });
    expect(demo.state.files['src/retry.rs']).toBe(retry);
    expect(demo.state.changed).toEqual({});
    expect(agentTexts(demo.state).at(-1)).toBe('Rejected. The file is unchanged.');
  });

  it('a decision is final: accepting or rejecting twice does nothing more', () => {
    const { demo, write } = proposed();
    const id = write().id;
    demo.dispatch({ type: 'accept', id });
    const after = demo.state.files['src/retry.rs'];
    const msgs = demo.state.chat.length;
    demo.dispatch({ type: 'accept', id });
    demo.dispatch({ type: 'reject', id });
    expect(demo.state.files['src/retry.rs']).toBe(after);
    expect(demo.state.chat).toHaveLength(msgs);
    expect(write().word).toBe('applied');
    demo.dispatch({ type: 'accept', id: 99999 });
    expect(demo.state.chat).toHaveLength(msgs);
  });

  it('two open proposals for the same file: the second fails on accept instead of corrupting the file', () => {
    const { demo, clock, prompt } = proposed();
    prompt('Fix the flaky retry test');
    clock.runAll();
    const [first, second] = tools(demo.state).filter((t) => t.name === 'workbench_write_file');
    expect([first!.review, second!.review]).toEqual([true, true]);
    demo.dispatch({ type: 'accept', id: first!.id });
    const after = demo.state.files['src/retry.rs'];
    demo.dispatch({ type: 'accept', id: second!.id });
    const [, s2] = tools(demo.state).filter((t) => t.name === 'workbench_write_file');
    expect(s2).toMatchObject({ state: 'error', word: 'error', review: false });
    expect(demo.state.files['src/retry.rs']).toBe(after);
    expect(demo.state.chat.at(-1)).toMatchObject({ kind: 'caption', text: 'no match for the old text; it may already be applied' });
  });

  it('new chat clears the conversation and any open review', () => {
    const { demo, write } = proposed();
    const id = write().id;
    demo.dispatch({ type: 'newChat' });
    expect(demo.state.chat).toEqual([]);
    expect(demo.state.chatTitle).toBe('New chat');
    demo.dispatch({ type: 'accept', id });
    expect(demo.state.files['src/retry.rs']).toBe(retry);
  });
});

describe('controls', () => {
  it('stop cancels the run and marks the chat stopped', () => {
    const { demo, clock, prompt } = make();
    prompt('Explain this repository');
    clock.advance(900);
    demo.dispatch({ type: 'stop' });
    expect(demo.state.running).toBe(false);
    expect(demo.state.chat.at(-1)).toMatchObject({ kind: 'stopped' });
    expect(clock.pending).toBe(0);
  });

  it('pause and reset', () => {
    const fresh = make().demo.state;
    const { demo, clock, prompt } = make();
    prompt('Fix the flaky retry test');
    clock.advance(600);
    demo.dispatch({ type: 'pause' });
    clock.runAll();
    expect(demo.state.running).toBe(true);
    demo.dispatch({ type: 'reset' });
    clock.runAll();
    expect({ ...demo.state, live: [] }).toEqual({ ...fresh, live: [] });
  });

  it('opens and closes files, falling back to the neighbour tab, and folds folders', () => {
    const { demo } = make();
    for (const p of ['README.md', 'web/config.ts']) demo.dispatch({ type: 'openFile', path: p });
    expect(demo.state.openFiles).toEqual(['src/retry.rs', 'README.md', 'web/config.ts']);
    demo.dispatch({ type: 'closeFile', path: 'web/config.ts' });
    expect(demo.state.active).toBe('README.md');
    demo.dispatch({ type: 'closeFile', path: 'src/retry.rs' });
    demo.dispatch({ type: 'closeFile', path: 'README.md' });
    expect(demo.state.active).toBeNull();
    demo.dispatch({ type: 'openFile', path: 'nope.rs' });
    expect(demo.state.active).toBeNull();
    expect(demo.state.tree.map((t) => t.key)).toEqual(['README.md', 'src', 'src/lib.rs', 'src/main.rs', 'src/retry.rs', 'web', 'web/config.ts']);
    demo.dispatch({ type: 'toggleFolder', folder: 'src' });
    expect(demo.state.tree.map((t) => t.key)).toEqual(['README.md', 'src', 'web', 'web/config.ts']);
  });
});

describe('command palette', () => {
  it('filters, moves, runs and closes; switching model changes a label only', () => {
    const { demo } = make();
    demo.dispatch({ type: 'paletteOpen' });
    expect(demo.state.palette.items.map((i) => i.label)).toEqual([
      'Switch model: Sample model 1', 'Switch model: Sample model 2', 'Toggle theme',
      'Open file: README.md', 'Open file: src/lib.rs', 'Open file: src/main.rs', 'Open file: src/retry.rs', 'Open file: web/config.ts',
    ]);
    expect(demo.state.palette.items[0]!.hint).toBe('current');
    demo.dispatch({ type: 'paletteQuery', text: 'MODEL 2' });
    expect(demo.state.palette.items.map((i) => i.label)).toEqual(['Switch model: Sample model 2']);
    demo.dispatch({ type: 'paletteRun' });
    expect(demo.state.model).toBe(1);
    expect(demo.state.palette.open).toBe(false);
    expect(demo.state.live.at(-1)).toBe('Model label set to Sample model 2. Replies stay scripted.');
  });

  it('opens a file, wraps the selection, and reports no matches', () => {
    const { demo } = make();
    demo.dispatch({ type: 'paletteOpen' });
    demo.dispatch({ type: 'paletteQuery', text: 'open file: web' });
    demo.dispatch({ type: 'paletteRun' });
    expect(demo.state.active).toBe('web/config.ts');
    demo.dispatch({ type: 'paletteOpen' });
    demo.dispatch({ type: 'paletteMove', delta: -1 });
    expect(demo.state.palette.sel).toBe(7);
    demo.dispatch({ type: 'paletteMove', delta: 1 });
    expect(demo.state.palette.sel).toBe(0);
    demo.dispatch({ type: 'paletteQuery', text: 'zzz' });
    expect(demo.state.palette.items).toEqual([]);
    demo.dispatch({ type: 'paletteRun' });
    expect(demo.state.palette.open).toBe(false);
  });

  it('toggles the theme for this demo only, until the page theme changes', () => {
    const { demo } = make({ pageTheme: 'light' });
    demo.dispatch({ type: 'paletteOpen' });
    demo.dispatch({ type: 'paletteQuery', text: 'theme' });
    demo.dispatch({ type: 'paletteRun' });
    expect(demo.state.theme).toBe('dark');
    demo.dispatch({ type: 'pageTheme', value: 'light' });
    expect(demo.state.theme).toBe('light');
    demo.dispatch({ type: 'pageTheme', value: 'dark' });
    expect(demo.state.theme).toBe('dark');
  });
});
