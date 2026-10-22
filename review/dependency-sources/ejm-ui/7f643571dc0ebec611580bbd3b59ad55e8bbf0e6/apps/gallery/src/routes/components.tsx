import {
  Badge, Band, Button, Checkbox, Dialog, Input, KeyHints, Menu, MenuContent, MenuItem, MenuLabel, MenuSeparator, MenuTrigger, Prompt,
  SectionHead, Select, Separator, StateBlock, Switch, Table, Tabs, Textarea, ToastProvider, Tooltip, useToast,
} from '@ejm/ui';
import { createContext, useContext, useId, useState, type ReactNode } from 'react';

// Every component and state in the primitives workstream, drawn twice: once under data-theme="light" and once under "dark".
// Only kit utilities. Overlays (tooltip, menu) portal into the theme wrapper so they take its theme; the dialog is native and
// stays inside it.

const Scope = createContext<HTMLElement | null>(null);

function Part({ name, note, children }: { name: string; note?: string; children: ReactNode }) {
  return (
    <section className="hairline-t grid gap-x-8 gap-y-4 py-8">
      <div>
        <h3 className="text-heading">{name}</h3>
        {note && <p className="mt-2 text-meta text-muted">{note}</p>}
      </div>
      <div className="flex min-w-0 flex-col gap-y-8">{children}</div>
    </section>
  );
}

function Row({ label, children }: { label?: string; children: ReactNode }) {
  return (
    <div>
      {label && <p className="pb-2 text-meta text-muted">{label}</p>}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">{children}</div>
    </div>
  );
}

const packages = [
  { name: '@ejm/core', size: 12, note: 'Headless logic for every target, with no framework' },
  { name: '@ejm/ui', size: 48, note: 'Components, hooks and layouts' },
  { name: '@ejm/tokens', size: 6, note: 'Brand colors and the generated theme' },
];
const columns = [
  { key: 'name', label: 'Package' },
  { key: 'size', label: 'Size (KB)' },
  { key: 'note', label: 'What it is', width: 28 },
];

function ToastDemo() {
  const { toast } = useToast();
  return (
    <Row>
      <Button variant="secondary" onClick={() => toast({ title: 'Saved', description: 'Your changes are live.' })}>Show a toast</Button>
      <Button variant="ghost" onClick={() => toast({ title: 'Project deleted', action: { label: 'Undo', altText: 'Undo the delete', onClick: () => {} } })}>With an action</Button>
    </Row>
  );
}

function Showcase({ theme }: { theme: 'light' | 'dark' }) {
  const id = useId();
  const container = useContext(Scope);
  const [dialog, setDialog] = useState(false);
  const [prompt, setPrompt] = useState('ejm init');
  return (
    <ToastProvider>
      <div className="page py-rhythm-sm">
        <p className="pb-4 text-meta text-muted">{theme} theme</p>
        <h2 className="text-title">{theme === 'light' ? 'Light theme' : 'Dark theme'}</h2>

        <Part name="Button" note="Pill shape, 44px on touch. The arrow moves 4px on hover.">
          <Row label="Variants">
            <Button>Primary</Button>
            <Button variant="secondary">Secondary</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="link">Link</Button>
            <Button disabled>Disabled</Button>
          </Row>
          <Row label="Arrow and sizes">
            <Button arrow>Get started</Button>
            <Button variant="link" arrow>Read the docs</Button>
            <Button size="sm" variant="secondary">Small</Button>
            <Button size="lg">Large</Button>
          </Row>
        </Part>

        <Part name="Fields" note="Native input, textarea and select. Switch and checkbox on Radix.">
          <div className="grid max-w-measure gap-x-8 gap-y-4 sm:grid-cols-2">
            <div className="space-y-3">
              <label htmlFor={`${id}-name`} className="text-meta text-muted">Name</label>
              <Input id={`${id}-name`} placeholder="Ada Lovelace" />
            </div>
            <div className="space-y-3">
              <label htmlFor={`${id}-email`} className="text-meta text-muted">Email, invalid</label>
              <Input id={`${id}-email`} type="email" defaultValue="ada@" aria-invalid="true" aria-describedby={`${id}-err`} />
              <p id={`${id}-err`} className="text-meta text-danger">Enter a full email address.</p>
            </div>
            <div className="space-y-3">
              <label htmlFor={`${id}-plan`} className="text-meta text-muted">Plan</label>
              <Select id={`${id}-plan`} defaultValue="pro"><option value="free">Free</option><option value="pro">Pro</option><option value="team">Team</option></Select>
            </div>
            <div className="space-y-3">
              <label htmlFor={`${id}-off`} className="text-meta text-muted">Disabled</label>
              <Input id={`${id}-off`} disabled defaultValue="Read only" />
            </div>
            <div className="space-y-3 sm:col-span-2">
              <label htmlFor={`${id}-msg`} className="text-meta text-muted">Message</label>
              <Textarea id={`${id}-msg`} placeholder="Describe what you are building." />
            </div>
          </div>
          <Row label="Switch">
            <Switch label="Off" />
            <Switch label="On" defaultChecked />
            <Switch label="Disabled" disabled />
          </Row>
          <Row label="Checkbox">
            <Checkbox label="Unchecked" />
            <Checkbox label="Checked" defaultChecked />
            <Checkbox label="Mixed" checked="indeterminate" />
            <Checkbox label="Disabled" disabled />
          </Row>
        </Part>

        <Part name="Badge and Separator" note="Tags are plain mono text. The rule is a hairline.">
          <Row>
            <Badge tone="strong">ejm-cli</Badge>
            <Badge separator="slash">Terminal agent</Badge>
            <Badge separator="dot">v0.4</Badge>
            <Badge tone="success">ok</Badge>
            <Badge tone="warn">warn</Badge>
            <Badge tone="danger">error</Badge>
            <Badge tone="info">info</Badge>
          </Row>
          <Separator />
        </Part>

        <Part name="Tooltip" note="Hover or focus the button.">
          <Row>
            <Tooltip content="Copy the install command" container={container}><Button variant="secondary" size="sm">Copy</Button></Tooltip>
          </Row>
        </Part>

        <Part name="Tabs" note="Arrows, Home and End. Only the current tab is in the tab order.">
          <Tabs
            label={`Products ${theme}`}
            tabs={[
              { id: 'cli', label: 'ejm-cli', panel: <p className="max-w-measure">A terminal agent that works in your repository.</p> },
              { id: 'ide', label: 'ejm-ide', panel: <p className="max-w-measure">An editor and engine for the same work.</p> },
              { id: 'harness', label: 'ejm-harness', panel: <p className="max-w-measure">A harness that checks the work.</p> },
            ]}
          />
        </Part>

        <Part name="Dialog, Menu, Toast" note="The only things that float, so the only ones with a shadow.">
          <Row>
            <Button variant="secondary" onClick={() => setDialog(true)}>Open a dialog</Button>
            <Menu>
              <MenuTrigger asChild><Button variant="secondary">Open a menu</Button></MenuTrigger>
              <MenuContent container={container}>
                <MenuLabel>Project</MenuLabel>
                <MenuItem>Rename</MenuItem>
                <MenuItem>Copy link</MenuItem>
                <MenuSeparator />
                <MenuItem disabled>Delete</MenuItem>
              </MenuContent>
            </Menu>
          </Row>
          <ToastDemo />
          <Dialog open={dialog} onOpenChange={setDialog} title="Rename the project" description="Pick a short name. You can change it later.">
            <label htmlFor={`${id}-dn`} className="text-meta text-muted">Name</label>
            <Input id={`${id}-dn`} defaultValue="ejm-ui" />
            <div className="mt-2 flex flex-wrap justify-end gap-3">
              <Button variant="ghost" onClick={() => setDialog(false)}>Cancel</Button>
              <Button onClick={() => setDialog(false)}>Save</Button>
            </div>
          </Dialog>
        </Part>

        <Part name="Table" note="Default, empty, loading and error. Scrolls inside its own region on a narrow screen.">
          <Table label={`Packages ${theme}`} columns={columns} rows={packages} />
          <Table label={`Empty ${theme}`} columns={columns} rows={[]} state="empty" action={{ label: 'Add a package' }} />
          <Table label={`Loading ${theme}`} columns={columns} rows={[]} state="loading" />
          <Table label={`Error ${theme}`} columns={columns} rows={[]} state="error" action={{ label: 'Try again' }} />
        </Part>

        <Part name="StateBlock" note="A word, one sentence, at most one action.">
          <StateBlock kind="empty" message="No projects yet. Create one to see it here." action={{ label: 'Create a project' }} />
          <StateBlock kind="loading" message="Loading your projects." />
          <StateBlock kind="error" message="Your projects could not be loaded. Check your connection and try again." action={{ label: 'Try again' }} />
        </Part>

        <Part name="Prompt and KeyHints" note="Idle, busy and error. Hints that do not fit drop from the end.">
          <Prompt value={prompt} onValueChange={setPrompt} placeholder="Type a command" label={`Command ${theme}`} />
          <Prompt value="ejm build" state="busy" label={`Busy command ${theme}`} />
          <Prompt value="ejm nope" state="error" message="Unknown command: nope. Try ejm help." label={`Failed command ${theme}`} />
          <KeyHints hints={[{ key: 'Enter', label: 'run' }, { key: 'Tab', label: 'complete' }, { key: 'Esc', label: 'cancel' }, { key: '?', label: 'help' }, { key: 'Ctrl+C', label: 'quit' }, { key: 'Ctrl+L', label: 'clear' }]} label={`Shortcuts ${theme}`} />
        </Part>
      </div>
    </ToastProvider>
  );
}

function Themed({ theme }: { theme: 'light' | 'dark' }) {
  const [el, setEl] = useState<HTMLElement | null>(null);
  return (
    <div ref={setEl} data-theme={theme} data-band="paper" className="band">
      <Scope.Provider value={el}>
        <Showcase theme={theme} />
      </Scope.Provider>
    </div>
  );
}

export default function ComponentsPage() {
  return (
    <>
      <Band tone="paper" aria-labelledby="components-title">
        <SectionHead purpose="page" kicker="ejm-ui / components" title="Every component, every state." as="h1" id="components-title">
          <p className="m-0">Each part below is drawn twice, under the light theme and then the dark one. They use only the kit utilities and read the theme contract.</p>
        </SectionHead>
      </Band>
      <Themed theme="light" />
      <Themed theme="dark" />
    </>
  );
}
