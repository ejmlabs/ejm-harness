import { formatDuration, formatNumber, type StatState } from '@ejm/core';
import { ActivityList, Band, ChartArea, ChartBar, ChartLine, Dashboard, Rule, SectionHead, Stat, Table, type ActivityItem, type DashboardProps } from '@ejm/ui';
import type { ReactNode } from 'react';

// The application kit: a dashboard, the three chart types and every empty, loading and error state, on a Paper band and an Ink band,
// in light and dark. Every figure here is SAMPLE data, invented for the demo. None of it is a real customer or a real metric.

const NOW = Date.UTC(2026, 9, 18, 14, 0, 0); // fixed, so the server and the browser print the same "5m ago"
const MIN = 60_000;
const none = () => {};

const days = ['Oct 5', 'Oct 6', 'Oct 7', 'Oct 8', 'Oct 9', 'Oct 10', 'Oct 11', 'Oct 12', 'Oct 13', 'Oct 14', 'Oct 15', 'Oct 16', 'Oct 17', 'Oct 18'];
const sessions = [
  { label: 'Started', values: [142, 156, 171, 168, 122, 98, 134, 163, 181, 177, 190, 184, 129, 143] },
  { label: 'Finished', values: [131, 149, 160, 158, 117, 92, 127, 151, 170, 168, 178, 171, 121, 135] },
  { label: 'Cancelled', values: [9, 11, 14, 12, 7, 5, 8, 10, 16, 12, 15, 17, 6, 9] },
  { label: 'Errored', values: [4, 6, 7, 9, 3, 2, 5, 8, 9, 6, 11, 13, 4, 7] },
];
const toolCalls = [
  { label: 'Succeeded', values: [3120, 3480, 3910, 3670, 2840] },
  { label: 'Retried', values: [310, 420, 380, 510, 260] },
  { label: 'Failed', values: [64, 91, 70, 138, 52] },
];
const hours = Array.from({ length: 24 }, (_, h) => `${String(h).padStart(2, '0')}:00`);
const tokens = [
  { label: 'Prompt tokens', short: 'Prompt', values: [38, 24, 18, 15, 17, 26, 52, 118, 204, 262, 281, 244, 198, 226, 271, 289, 262, 231, 172, 124, 96, 78, 61, 47] },
  { label: 'Completion tokens', short: 'Completion', values: [11, 8, 6, 5, 6, 9, 17, 36, 61, 78, 86, 73, 60, 69, 82, 88, 79, 70, 52, 38, 29, 23, 18, 14] },
];
const thousands = (n: number) => `${formatNumber(n)}k`;

const toolColumns = [
  { key: 'tool', label: 'Tool', width: 18 },
  { key: 'calls', label: 'Calls', align: 'right' as const },
  { key: 'errors', label: 'Errors', align: 'right' as const },
  { key: 'median', label: 'Median', align: 'right' as const },
];
const toolRows = [
  ['read_file', 6120, 41, 38],
  ['search_text', 4210, 29, 210],
  ['edit_file', 3380, 118, 95],
  ['run_command', 2760, 163, 4_800],
  ['list_dir', 1240, 6, 22],
  ['fetch_page', 696, 88, 1_350],
].map(([tool, calls, errors, median]) => ({ tool: tool as string, calls: formatNumber(calls as number), errors: formatNumber(errors as number), median: formatDuration(median as number) }));

const feed: ActivityItem[] = [
  { id: 'a1', title: 'Session sess_4831 finished', detail: 'planner, 214 tool calls', at: NOW - 3 * MIN, status: 'ok' },
  { id: 'a2', title: 'run_command timed out after 30s', detail: 'sess_4829', at: NOW - 12 * MIN, status: 'error' },
  { id: 'a3', title: 'edit_file hit the retry limit', detail: 'sess_4827, 3 of 3 tries', at: NOW - 47 * MIN, status: 'warn' },
  { id: 'a4', title: 'Session sess_4826 is waiting for approval', detail: 'reviewer', at: NOW - 95 * MIN, status: 'pending' },
  { id: 'a5', title: 'Replay of sess_4790 started', detail: 'offline, no model calls', at: NOW - 6 * 3600_000, status: 'info' },
  { id: 'a6', title: 'Session sess_4818 finished', detail: 'planner, 96 tool calls', at: NOW - 29 * 3600_000, status: 'ok' },
];

const stats: DashboardProps['stats'] = [
  { label: 'sessions', figure: '1,284', delta: { text: '+8.2% vs last week', direction: 'up', tone: 'good' } },
  { label: 'tool calls', figure: '18,406', delta: { text: '+12.4% vs last week', direction: 'up', tone: 'neutral' } },
  { label: 'tokens used', figure: '41.2M', delta: { text: '+3.1% vs last week', direction: 'up', tone: 'neutral' } },
  { label: 'median run time', figure: formatDuration(72_000), delta: { text: '-6.0% vs last week', direction: 'down', tone: 'good' } },
];

const dashboard: DashboardProps = {
  title: 'Agent activity',
  period: 'Oct 5 to Oct 18, 2026',
  sample: true,
  stats,
  chart: { title: 'Sessions per day', description: 'Sessions by how they ended', categories: days, series: sessions, categoryLabel: 'Day', unit: 'sessions' },
  table: { title: 'Busiest tools', columns: toolColumns, rows: toolRows },
  activity: { title: 'Recent activity', items: feed, now: NOW },
};

function Pair({ band, children }: { band: 'paper' | 'ink'; children: ReactNode }) {
  return (
    <div className="grid lg:grid-cols-12">
      {(['light', 'dark'] as const).map(theme => (
        <div key={theme} data-theme={theme} data-band={band} className="band page-inset py-rhythm-sm lg:col-span-6">
          <p className="text-meta m-0 mb-8 text-muted">{theme} theme / {band} band</p>
          <div className="flex flex-col gap-10">{children}</div>
        </div>
      ))}
    </div>
  );
}

const Charts = (
  <>
    <ChartLine title="Sessions per day" description="Line, four series: marker shape and dash pattern tell them apart" categories={days} series={sessions} categoryLabel="Day" unit="sessions" />
    <ChartBar title="Tool calls by day" description="Bar, three series: each has its own fill pattern" categories={['Mon', 'Tue', 'Wed', 'Thu', 'Fri']} series={toolCalls} categoryLabel="Day" unit="calls" />
    <ChartArea title="Tokens per hour" description="Area, two series, in thousands of tokens" categories={hours} series={tokens} categoryLabel="Hour" unit="thousands of tokens" format={thousands} />
  </>
);

const statStates: StatState[] = ['default', 'loading', 'error', 'empty'];

const States = (
  <>
    <div>
      <Rule label="Stat" />
      <div className="mt-6 grid grid-cols-2 gap-x-6 gap-y-8">
        {statStates.map(s => (
          <Stat key={s} state={s} figure="1,284" label={`sessions, ${s}`} size="title" />
        ))}
      </div>
    </div>
    <div>
      <Rule label="Activity list" />
      <div className="mt-6 flex flex-col gap-10">
        <ActivityList items={[]} now={NOW} label="Activity, empty" state="empty" action={{ label: 'Start a session' }} onAction={none} />
        <ActivityList items={[]} now={NOW} label="Activity, loading" state="loading" />
        <ActivityList items={[]} now={NOW} label="Activity, error" state="error" action={{ key: 'r', label: 'Try again' }} onAction={none} />
      </div>
    </div>
    <div>
      <Rule label="Chart" />
      <div className="mt-6 flex flex-col gap-10">
        <ChartLine title="Empty chart" categories={[]} series={[]} action={{ label: 'Widen the period' }} onAction={none} />
        <ChartBar title="Loading chart" categories={[]} series={[]} state="loading" />
        <ChartArea title="Error chart" categories={[]} series={[]} state="error" action={{ label: 'Try again' }} onAction={none} />
      </div>
    </div>
    <div>
      <Rule label="Table" />
      <div className="mt-6 flex flex-col gap-10">
        {(['empty', 'loading', 'error'] as const).map(s => (
          <Table key={s} columns={toolColumns} rows={[]} state={s} label={`Tools, ${s}`} action={s === 'loading' ? undefined : { label: s === 'empty' ? 'Clear the filter' : 'Try again' }} onAction={none} />
        ))}
      </div>
    </div>
  </>
);

export default function App() {
  return (
    <>
      <Band tone="paper" aria-labelledby="app-title">
        <SectionHead purpose="page" kicker="ejm-ui / application kit" title="Figures, charts and calm states." as="h1" id="app-title">
          <p className="m-0">A dashboard, three chart types and every empty, loading and error state, with no boxes. All data on this page is sample data, invented for the demo.</p>
        </SectionHead>
      </Band>

      <Band tone="paper" aria-labelledby="dash-title">
        <SectionHead kicker="01 / Dashboard" title="One row of figures, then the detail." id="dash-title" />
        <Dashboard {...dashboard} as="h3" className="mt-16" />
      </Band>

      <Band tone="ink" aria-labelledby="dash-ink-title">
        <SectionHead kicker="02 / Dashboard on Ink" title="The same dashboard on a dark band." id="dash-ink-title" />
        <Dashboard {...dashboard} as="h3" className="mt-16" />
      </Band>

      <Band tone="mist" flush column={false} aria-labelledby="charts-title">
        <div className="page py-rhythm-sm">
          <SectionHead kicker="03 / Charts" title="Line, bar and area." id="charts-title">
            <p className="m-0">Each series has a hue, a marker or fill pattern, a name in the legend and a table of its values. The dashboard above shows a chart on Ink.</p>
          </SectionHead>
        </div>
        <Pair band="paper">{Charts}</Pair>
      </Band>

      <Band tone="paper" flush column={false} aria-labelledby="states-title">
        <div className="page py-rhythm-sm">
          <SectionHead kicker="04 / States" title="Every list, table and panel has a sentence." id="states-title">
            <p className="m-0">Empty, loading and error each say one plain sentence and offer one action where one exists.</p>
          </SectionHead>
        </div>
        <Pair band="paper">{States}</Pair>
        <Pair band="ink">{States}</Pair>
      </Band>

      <Band tone="mist" aria-labelledby="dash-states-title">
        <SectionHead kicker="05 / Dashboard states" title="The whole dashboard, and parts of it." id="dash-states-title" />
        <div className="mt-16 flex flex-col gap-10">
          <Dashboard {...dashboard} as="h3" title="Loading" state="loading" />
          <Dashboard {...dashboard} as="h3" title="Error" state="error" action={{ key: 'r', label: 'Try again' }} onAction={none} />
          <Dashboard {...dashboard} as="h3" title="Empty" state="empty" action={{ label: 'Start a session' }} onAction={none} />
          <Dashboard
            {...dashboard}
            as="h3"
            title="Partly failed"
            stats={stats.map((s, i) => (i === 2 ? { ...s, state: 'error' as const } : s))}
            chart={{ ...dashboard.chart, state: 'error', action: { label: 'Try again' }, onAction: none }}
            table={{ ...dashboard.table, rows: [], state: 'empty', action: { label: 'Clear the filter' }, onAction: none }}
            activity={{ ...dashboard.activity, state: 'loading' }}
          />
        </div>
      </Band>
    </>
  );
}
