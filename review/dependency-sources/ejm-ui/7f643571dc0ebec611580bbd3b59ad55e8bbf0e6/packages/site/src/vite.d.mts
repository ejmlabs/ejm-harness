import type { Plugin } from 'vite';
import type { HeadersOptions } from './headers.mjs';
import type { Project, ProjectId } from './projects.mjs';

export interface EjmSiteOptions {
  project: ProjectId | Project;
  /** Options for `_headers`, or `false` to leave the file to the site. */
  headers?: HeadersOptions | false;
}
/** Files the plugin serves and writes at the site root: published name to absolute path. */
export function sharedFiles(project: Project): Record<string, string>;
/** Tags as HTML strings. An optional content revision refreshes the icon and manifest URLs. */
export function headTags(base?: string, version?: string): string[];
/** Adds shared assets and requires shared design ownership in consumer source. */
export function ejmSite(options: EjmSiteOptions): Plugin;
