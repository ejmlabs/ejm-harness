import { useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type RefObject } from 'react';
import type { IdeAction } from '../ide';
import { CodeLines } from './code';
import { cx } from './cx';
import { ideFrame } from './frames/ide';
import type { IdeFrame } from './frames';
import { ChatLine } from './ide-parts';
import { IdeIcon } from './ide-icons';
import type { ViewProps } from './CliView';

export interface IdeViewProps extends ViewProps {
  frame?: IdeFrame;
  /** Retained for existing composition callers. The native workspace keeps its desktop layout. */
  layout?: 'stack' | 'split' | 'auto';
  passive?: boolean;
}

interface IdeScreenProps {
  frame: IdeFrame;
  layout?: IdeViewProps['layout'];
  dispatch?: (action: IdeAction) => void;
  running?: boolean;
  folders?: Record<string, boolean>;
  flash?: number[];
  chatLog?: RefObject<HTMLDivElement | null>;
  panel?: RefObject<HTMLDivElement | null>;
  command?: RefObject<HTMLButtonElement | null>;
  /** Keep product content readable while the external showcase owns input. */
  passive?: boolean;
}

/** Presentation adapter for App, EditorWorkspace and ChatPane in ejm-ide. */
export function IdeScreen({ frame, dispatch, running = false, folders = {}, flash, chatLog, panel, command, passive = false }: IdeScreenProps) {
  const id = useId();
  const field = useRef<HTMLTextAreaElement>(null);
  const ownChat = useRef<HTMLDivElement>(null);
  const ownPanel = useRef<HTMLDivElement>(null);
  const messageLog = chatLog ?? ownChat;
  const codePanel = panel ?? ownPanel;
  const tabs = useRef<Record<string, HTMLButtonElement | null>>({});
  const [project, setProject] = useState(true);
  const [chat, setChat] = useState(true);
  const active = frame.active;
  useLayoutEffect(() => {
    if (passive && messageLog.current) messageLog.current.scrollTop = messageLog.current.scrollHeight;
  }, [passive, frame.chat, messageLog]);
  useLayoutEffect(() => {
    if (!passive || !codePanel.current) return;
    const element = codePanel.current;
    const line = flash?.[0];
    const target = line === undefined ? null : element.querySelector<HTMLElement>(`[data-line="${line + 1}"]`);
    if (target) element.scrollTop += target.getBoundingClientRect().top - element.getBoundingClientRect().top - 24;
    else element.scrollTop = 0;
  }, [passive, active, flash?.[0], codePanel]);
  const tabKey = (event: KeyboardEvent, path: string) => {
    if (!dispatch) return;
    const index = frame.openFiles.indexOf(path);
    const count = frame.openFiles.length;
    let next = index;
    if (event.key === 'ArrowRight') next++;
    else if (event.key === 'ArrowLeft') next--;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = count - 1;
    else if (event.key === 'Delete') dispatch({ type: 'closeFile', path });
    else return;
    event.preventDefault();
    const target = frame.openFiles[(next + count) % count];
    if (event.key !== 'Delete' && target) {
      dispatch({ type: 'openFile', path: target });
      tabs.current[target]?.focus();
    }
  };
  const submit = () => {
    const text = field.current?.value.trim();
    if (!text || running || !dispatch) return;
    dispatch({ type: 'prompt', text });
    if (field.current) field.current.value = '';
  };
  return (
    <section className="ejm-ide-viewport" tabIndex={0} role="region" aria-label="IDE workspace. Scroll horizontally to view the editor and assistant.">
      <div className="ejm-ide" onKeyDown={(event) => {
        if (dispatch && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
          event.preventDefault(); dispatch({ type: 'paletteOpen' });
        }
      }}>
        <header className="edi-titlebar">
          <span className="edi-window-controls" aria-hidden="true"><i /><i /><i /></span>
          <button type="button" className="edi-workspace" disabled><IdeIcon name="folder-open" />harbor<IdeIcon name="chevron-down" /></button>
          <span className="edi-branch"><IdeIcon name="git-branch" />main</span>
          <span className="edi-spacer" />
          <button ref={command} type="button" className="edi-search" aria-label="Search files and commands" aria-haspopup="dialog" disabled={!dispatch} onClick={() => dispatch?.({ type: 'paletteOpen' })}><IdeIcon name="search" /><span>Search…</span><kbd>⌘ K</kbd></button>
          <button type="button" className="edi-icon" aria-label="Toggle project panel" aria-pressed={project} disabled={!dispatch} onClick={() => setProject(!project)}><IdeIcon name="panel-left" /></button>
          <button type="button" className="edi-icon" aria-label="Toggle assistant panel" aria-pressed={chat} disabled={!dispatch} onClick={() => setChat(!chat)}><IdeIcon name="panel-right" /></button>
        </header>
        <div className="edi-body">
          <nav className="edi-rail" aria-label="Activity">
            <button type="button" className="edi-icon edi-active" aria-label="Project" disabled={!dispatch} onClick={() => setProject(!project)}><IdeIcon name="files" /></button>
            <button type="button" className="edi-icon" aria-label="Search" disabled={!dispatch} onClick={() => dispatch?.({ type: 'paletteOpen' })}><IdeIcon name="search" /></button>
            <button type="button" className="edi-icon" aria-label="Source control" disabled><IdeIcon name="git-branch" /></button>
            <button type="button" className="edi-icon" aria-label="Extensions" disabled><IdeIcon name="blocks" /></button>
            <span className="edi-spacer" />
            <button type="button" className="edi-icon" aria-label="Terminal" disabled><IdeIcon name="terminal" /></button>
            <button type="button" className="edi-icon" aria-label="Settings" disabled><IdeIcon name="settings" /></button>
          </nav>
          {project && <aside className="edi-project" aria-label="Project files">
            <div className="edi-panel-heading">Project <span className="edi-spacer" /><span aria-hidden="true">···</span></div>
            <div className="edi-project-name"><IdeIcon name="chevron-down" />harbor</div>
            <ul aria-label="Files" className="edi-tree">
              {frame.tree.map((entry) => <li key={entry.key}>
                <button type="button" disabled={!dispatch} aria-expanded={entry.dir ? folders[entry.key] ?? true : undefined} aria-current={!entry.dir && entry.key === active ? 'true' : undefined}
                  onClick={() => dispatch?.(entry.dir ? { type: 'toggleFolder', folder: entry.key } : { type: 'openFile', path: entry.key })}
                  className={cx('edi-file', entry.key === active && 'edi-selected')} style={{ paddingInlineStart: 14 + entry.level * 14 }}>
                  <IdeIcon name={entry.dir ? folders[entry.key] === false ? 'chevron-right' : 'chevron-down' : 'file-code'} />{entry.key.split('/').pop()}
                </button>
              </li>)}
            </ul>
          </aside>}
          <section className="edi-editor">
            <div role="tablist" aria-label="Open files" className="edi-tabs">
              {frame.openFiles.map((path) => <div key={path} className={cx('edi-tab', path === active && 'edi-active')}>
                <button ref={(element) => { tabs.current[path] = element; }} type="button" role="tab" id={`${id}-tab-${path}`} aria-selected={path === active} aria-controls={`${id}-editor`} tabIndex={path === active ? 0 : -1}
                  disabled={!dispatch} onClick={() => dispatch?.({ type: 'openFile', path })} onKeyDown={(event) => tabKey(event, path)} aria-keyshortcuts="Delete"><IdeIcon name="file-code" />{path.split('/').pop()}<span aria-hidden="true" className="edi-close" onClick={(event) => { event.stopPropagation(); dispatch?.({ type: 'closeFile', path }); }}><IdeIcon name="x" /></span></button>
              </div>)}
            </div>
            <div className="edi-breadcrumbs">{active ? active.replaceAll('/', ' › ') : 'No file open'}</div>
            <div ref={codePanel} id={`${id}-editor`} role="tabpanel" aria-labelledby={active ? `${id}-tab-${active}` : undefined} aria-label={active || 'Editor'} tabIndex={0} className="edi-code">
              {active && <CodeLines code={frame.file.text} lang={frame.file.lang} changed={frame.changed} flash={flash} />}
            </div>
          </section>
          {chat && <section className="edi-chat" aria-label={`${frame.agent} chat`}>
            <header className="edi-chat-header"><strong className="truncate">{frame.chatTitle === 'New chat' ? frame.agent : frame.chatTitle}</strong><span className="edi-connection" /><span className="edi-spacer" />
              <button type="button" className="edi-icon" aria-label="Chat history" disabled><IdeIcon name="history" /></button>
              <button type="button" className="edi-icon" aria-label="New chat" disabled={!dispatch} onClick={() => dispatch?.({ type: 'newChat' })}><IdeIcon name="square-pen" /></button>
            </header>
            <div ref={messageLog} role="log" aria-live="off" aria-label={`${frame.agent} messages`} tabIndex={0} className="edi-messages">
              {frame.chat.length === 0 && <p className="edi-empty">Ask a question or describe a change.</p>}
              {frame.chat.map((item) => <ChatLine key={item.id} item={item} agent={frame.agent} passive={passive} />)}
            </div>
            <form className="edi-composer" onSubmit={(event) => { event.preventDefault(); submit(); }}>
              <textarea ref={field} {...(frame.composer !== undefined && { value: frame.composer, readOnly: true })} aria-label={`Message ${frame.agent}`} placeholder="Ask a question or describe a change…" rows={3} disabled={!dispatch} autoComplete="off" spellCheck={false}
                onKeyDown={(event) => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); submit(); } }} />
              <div className="edi-composer-toolbar">
                <button type="button" className="edi-icon" aria-label="Add context" disabled><span aria-hidden="true">@</span></button>
                <button type="button" className="edi-model" aria-label={`Model: ${frame.model}`} disabled><IdeIcon name="box" /><IdeIcon name="chevron-down" /></button>
                <span className="edi-spacer" />
                <span role="img" className="edi-auto-review" title="Writes are reviewed automatically" aria-label="Writes are reviewed automatically"><IdeIcon name="shield-check" /></span>
                {running ? <button type="button" className="edi-icon edi-send" aria-label="Stop" disabled={!dispatch} onClick={() => dispatch?.({ type: 'stop' })}><IdeIcon name="square" /></button>
                  : <button type="submit" className="edi-icon edi-send" aria-label="Send" disabled={!dispatch}><IdeIcon name="arrow-up" /></button>}
              </div>
            </form>
          </section>}
        </div>
        <footer className="edi-statusbar"><span><IdeIcon name="git-branch" />main</span><span>0 errors, 0 warnings</span><span className="edi-spacer" /><span>UTF-8</span><span>{frame.file.lang === 'rust' ? 'Rust' : frame.file.lang === 'ts' ? 'TypeScript' : 'Text'}</span><span>LF</span></footer>
      </div>
    </section>
  );
}

/** Static HTML for the same workspace and inline tool results as the island. */
export function IdeView({ frame = ideFrame, label = 'ejm-ide', className, passive = false }: IdeViewProps) {
  return <div role="group" aria-label={label} className={className}><IdeScreen frame={frame} passive={passive} /></div>;
}
