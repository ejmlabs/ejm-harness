import type { CalloutKind } from '@ejm/core';
import type { ContentsItem } from './Contents';

/** The build-time Markdown compiler emits only this JSON-compatible data. */
export type DocInline = string | ['code', string] | ['strong' | 'em' | 'del', DocInline[]] | ['br'] | ['a', string, boolean, DocInline[]];
export interface DocCode { type: 'code'; code: string; lang?: string; label?: string }
export interface DocCodeTabs { type: 'tabs'; label?: string; tabs: { label: string; code: string; lang?: string; codeLabel?: string }[] }
export type DocDataBlock =
  | { type: 'p'; children: DocInline[] }
  | { type: 'list'; ordered: boolean; start?: number; items: DocDataBlock[][] }
  | DocCode
  | DocCodeTabs
  | { type: 'callout'; kind: CalloutKind; blocks: DocDataBlock[] }
  | { type: 'table'; caption: string; heads: string[]; rows: DocInline[][][] }
  | { type: 'h3'; id: string; title: string };
export interface DocMetadata { slug: string; group: string; title: string; lead: string; description: string; order?: number }
export interface SerializedDocPage extends DocMetadata { sections: { id: string; title: string; blocks: DocDataBlock[] }[] }
export interface DocProseRun { type: 'prose'; blocks: DocDataBlock[] }
export interface FullDocPage extends DocMetadata { sections: { id: string; title: string; items: (DocProseRun | DocDataBlock)[] }[] }
export interface ClientDocPage extends DocMetadata {
  contents: ContentsItem[];
  sections: { id: string; items: ({ type: 'prose' } | DocDataBlock)[] }[];
}
