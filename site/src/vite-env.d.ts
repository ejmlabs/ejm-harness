/// <reference types="vite/client" />

declare module "*.md" {
  export const full: import("@ejm/ui/docs").FullDocPage;
  export const renderBlock: import("@ejm/ui/docs").DocsPageProps["renderBlock"];
  const client: import("@ejm/ui/docs").ClientDocPage;
  export default client;
}

declare module "virtual:docs-meta" {
  const pages: {
    title: string;
    description: string;
    group: string;
    href: string;
    slug: string;
    order: number;
  }[];
  export default pages;
}
