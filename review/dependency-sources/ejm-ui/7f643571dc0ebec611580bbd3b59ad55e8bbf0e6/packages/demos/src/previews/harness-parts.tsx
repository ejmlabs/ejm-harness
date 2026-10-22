import type { HarnessFrame } from './frames';

/** Terminal colors and type, without a titlebar or demo controls. */
export const harnessTerminalClass = 'scheme-page flex h-full min-h-[16em] min-w-0 flex-col overflow-hidden bg-ui-editor p-4 font-mono text-[length:var(--ejm-window-text,0.8125rem)] leading-normal text-ui-text';

const governorKeys = ['running', 'queued', 'heavy_running', 'reserved_cpu_millis', 'reserved_memory_bytes', 'reserved_output_bytes', 'reserved_processes'];
const harnessKeys = ['active_agents', 'model_requests', 'waiting_model_requests'];

function counts(value: unknown, keys: readonly string[]): value is Record<string, number> {
  if (!value || typeof value !== 'object' || Object.keys(value).length !== keys.length) return false;
  const fields = value as Record<string, unknown>;
  return keys.every(key => typeof fields[key] === 'number' && Number.isFinite(fields[key]));
}

function governorLines(value: Record<string, number>): string[] {
  return [
    `jobs: running ${value.running} | queued ${value.queued} | heavy ${value.heavy_running}`,
    `reserved: cpu ${value.reserved_cpu_millis}m | memory ${value.reserved_memory_bytes} B`,
    `reserved: output ${value.reserved_output_bytes} B | processes ${value.reserved_processes}`,
  ];
}

/** Format known recorded snapshots only. Unknown fields and ordinary stdout stay intact. */
function recordedLine(line: string): string[] {
  if (!line.startsWith('{')) return [line];
  try {
    const value: unknown = JSON.parse(line);
    if (counts(value, governorKeys)) return governorLines(value);
    if (value && typeof value === 'object') {
      const { tools, ...state } = value as Record<string, unknown>;
      if (counts(state, harnessKeys) && counts(tools, governorKeys)) return [
        `agents: ${state.active_agents} | model requests ${state.model_requests} | waiting ${state.waiting_model_requests}`,
        ...governorLines(tools),
      ];
    }
  } catch { /* Preserve output that is not a recorded JSON snapshot. */ }
  return [line];
}

/** A readable terminal presentation of retained stdout. Session data is never rewritten. */
export function HarnessOutput({ sessions }: { sessions: HarnessFrame[] }) {
  return (
    <div className="min-w-0 whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
      {sessions.map((session, index) => (
        <div key={index} className={index ? 'mt-[0.8em]' : undefined}>
          <p className="m-0">{'$ ' + session.command}</p>
          {session.output.length > 0 && <pre className="m-0 whitespace-pre-wrap break-words font-[inherit] text-[length:inherit] [overflow-wrap:anywhere]">{session.output.flatMap(recordedLine).join('\n')}</pre>}
        </div>
      ))}
    </div>
  );
}
