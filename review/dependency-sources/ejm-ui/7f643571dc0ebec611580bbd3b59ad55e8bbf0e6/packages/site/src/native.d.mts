export interface NativeDocument { name: string; html: string }
export interface NativeDesignOptions {
  root: string;
  /** Authored source directories. Build and test fixtures stay outside these roots. */
  source?: string[];
  /** Same-origin URL at which the public @ejm/ui/static.css bytes are served. */
  stylesheetHref: string;
  /** Generated route and conditional-state documents, checked after template composition. */
  documents?: NativeDocument[];
}
export function checkNativeDesign(options: NativeDesignOptions): { files: string[]; documents: number };
