import type { ClientDocPage, DocMetadata, FullDocPage, SerializedDocPage } from '@ejm/ui/docs';
export interface DocLink { href: string; external: boolean }
export interface ParseDocOptions {
  /** Consumer-owned route and repository policy. The compiler checks safety before and after this callback. */
  link?: (href: string) => DocLink;
  metadata?: Partial<DocMetadata>;
  requireFrontMatter?: boolean;
  requiredMetadata?: readonly (keyof DocMetadata)[];
  requireExplicitIds?: boolean;
}
/** GitHub-compatible slug without duplicate tracking. parseDoc tracks duplicate headings per page. */
export function slug(text: string): string;
export function parseDoc(source: string, options?: ParseDocOptions): SerializedDocPage;
/** Keep prose in server HTML while controls remain live. requiresRich flags nested interactive blocks. */
export function splitDoc(page: SerializedDocPage): { full: FullDocPage; client: ClientDocPage; requiresRich: boolean };
