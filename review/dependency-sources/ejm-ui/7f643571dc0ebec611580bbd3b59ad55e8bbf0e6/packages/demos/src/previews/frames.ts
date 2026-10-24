/**
 * Reviewed excerpts from the bundled native previews. The recordings keep the full
 * workflow; each static and completed view keeps a useful compact result. See
 * PROVENANCE.md for capture facts, authored narration and verification limits.
 * One file per product keeps other product data out of a one-product entry.
 */
import type { CliBlock } from '../cli';
import type { ChatItem, TreeEntry } from '../ide';
import { cliFrame } from './frames/cli';
import { harnessFrame } from './frames/harness';
import { ideFrame } from './frames/ide';

export interface CliFrame {
  provider: string;
  cwd: string;
  title: string;
  sessionId: string;
  write: boolean;
  shell?: boolean;
  agents: number;
  models: number;
  tools: number;
  blocks: CliBlock[];
  /** Key and label pairs, from labels.json. */
  hints: [string, string][];
}

export interface IdeFrame {
  agent: string;
  chatTitle: string;
  /** The model label. Replies are scripted. */
  model: string;
  openFiles: string[];
  active: string;
  tree: TreeEntry[];
  file: { path: string; lang: string; text: string };
  /** Zero-based lines changed by the accepted edit. */
  changed: number[];
  chat: ChatItem[];
  composer?: string;
}

export interface HarnessFrame {
  command: string;
  output: string[];
}

export interface Frames {
  cli: CliFrame;
  ide: IdeFrame;
  harness: HarnessFrame;
}

export { cliFrame, ideFrame, harnessFrame };

/** All three. A view imports its own frame instead, so a page that shows one product does not bundle the others. */
export const frames: Frames = { cli: cliFrame, ide: ideFrame, harness: harnessFrame };
