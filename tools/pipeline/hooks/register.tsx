// Wires the pipeline to Claude Code: the tools of the liaison, the events of the
// subagents, the timer, the pane and the status line. Decisions are in
// machine.ts, the carrying out of them in driver.ts, the server and git in
// forge.ts.
import type { Register } from 'claude-code'

import { createPipeline, type Input, type Pipeline, type Ports } from './driver'
import { makeForge } from './forge'
import { DEVELOPERS, type Developer, POLL_MS, type State } from './model'
import { describeAll, summary, viewRows } from './view'

const PANE = 'pipeline'
const TOOL = (name: string): string => `mcp__pipeline__${name}`
const STORE_KEY = 'tasks'
const rows = { plugin: 'pipeline', key: 'tasks' } as const

const TOOLS = [
  {
    name: 'pipeline_start',
    description:
      'Start a task of the development pipeline. The developer starts at once, or when every task in "after" is merged. The pipeline then runs the developer, the full check, the review, the merge and the cleanup by itself and reports to you.',
    inputSchema: {
      type: 'object',
      properties: {
        slug: { type: 'string', description: 'Short name of the task; the branch is task/<slug>' },
        developer: { type: 'string', enum: [...DEVELOPERS] },
        brief: { type: 'string', description: 'The task as for a developer: design, files, acceptance criteria, out of scope' },
        after: { type: 'array', items: { type: 'string' }, description: 'Slugs of tasks that must be merged first' },
      },
      required: ['slug', 'developer', 'brief'],
    },
  },
  {
    name: 'pipeline_status',
    description: 'Every task of the pipeline: stage, pull request, head, CI state, developer and agent id, rounds, last event.',
    inputSchema: { type: 'object', properties: {} },
  },
  {
    name: 'pipeline_send',
    description:
      'A message to the current developer of a task: your answer to its question or a decision on an addition. Resumes a task that waits for you (a blocked task continues on a new developer-3).',
    inputSchema: {
      type: 'object',
      properties: { slug: { type: 'string' }, text: { type: 'string' } },
      required: ['slug', 'text'],
    },
  },
  {
    name: 'pipeline_stop',
    description:
      "Stop one task, or all (the user's order): every running agent of it is told to commit and push what is worth keeping and stop, nothing new starts for it, and it is marked blocked.",
    inputSchema: {
      type: 'object',
      properties: { slug: { type: 'string', description: 'Omit to stop every task' }, reason: { type: 'string' } },
      required: ['reason'],
    },
  },
] as const

const str = (v: unknown): string => (typeof v === 'string' ? v : '')

export const register: Register = on => {
  let pipeline: Pipeline | undefined

  on('session.start', async ($, e, next) => {
    const root = await $.session.root()
    const ports: Ports = {
      now: () => $.clock.now(),
      load: async () => {
        const v = await $.store.get(STORE_KEY)
        return v && typeof v === 'object' && Array.isArray((v as State).tasks) ? (v as State) : undefined
      },
      save: state => $.store.set(STORE_KEY, state),
      show: state => {
        $.ui.status(summary(state))
        void $.state.set(rows, viewRows(state)).catch(() => undefined)
      },
      forge: makeForge((argv, init) => $.process.run(argv, init), { root, sleep: ms => $.clock.sleep(ms) }),
      agents: {
        spawn: async a => {
          const r = await $.agent.spawn({ prompt: a.prompt, subagentType: a.tier, description: a.description })
          return r.deny !== undefined ? { deny: r.deny } : { agentId: r.agentId }
        },
        send: async (agentId, text) => {
          try {
            return (await $.session.send({ to: { agentId }, text })).isDelivered
          } catch {
            return false
          }
        },
        append: async (agentId, text) => {
          try {
            await $.session.append({ agentId, message: { type: 'user', content: [{ type: 'text', text }] } })
            return true
          } catch {
            return false
          }
        },
      },
      notify: text => {
        void $.prompt.submit({ text }).catch(() => undefined)
      },
      sleep: ms => $.clock.sleep(ms),
      log: text => $.ui.log(`pipeline: ${text}`),
    }
    const p = createPipeline(ports)
    pipeline = p
    for (const t of TOOLS) await $.tool.register({ ...t, isDeferred: false })
    await $.command.register({ name: 'pipeline', description: 'Show the tasks of the development pipeline' })
    await p.start()
    $.clock.every(POLL_MS, () => {
      p.tick().catch(err => $.ui.log(`pipeline: tick failed: ${String(err)}`))
    })
    return next(e)
  })

  const served = (name: string, handle: (p: Pipeline, args: Record<string, unknown>) => Promise<string>): void => {
    on('tool.call', { tool: new RegExp(`^${TOOL(name)}$`) }, async (_$, e) => {
      if (e.agentId !== undefined) return { deny: 'the pipeline tools are for the liaison, not for subagents' }
      if (!pipeline) return { deny: 'the pipeline is not started yet' }
      try {
        return { result: await handle(pipeline, e as unknown as Record<string, unknown>) }
      } catch (err) {
        return { deny: String(err instanceof Error ? err.message : err) }
      }
    }).catch(() => ({ deny: 'the pipeline tool failed' }))
  }

  const reply = async (p: Pipeline, ev: Input): Promise<string> => {
    const r = await p.dispatch(ev)
    if (r.error !== undefined) throw new Error(r.error)
    return r.reply ?? 'ok'
  }

  served('pipeline_start', (p, a) =>
    reply(p, {
      type: 'start',
      slug: str(a.slug),
      developer: str(a.developer) as Developer,
      brief: str(a.brief),
      after: Array.isArray(a.after) ? a.after.map(str).filter(s => s !== '') : [],
    }),
  )
  served('pipeline_status', async p => describeAll(p.state()))
  served('pipeline_send', (p, a) => reply(p, { type: 'send', slug: str(a.slug), text: str(a.text) }))
  served('pipeline_stop', (p, a) =>
    reply(p, { type: 'stop', slug: str(a.slug) || undefined, reason: str(a.reason) || 'no reason given' }),
  )

  on('turn.complete', async ($, e, next) => {
    const done = await next(e)
    if (pipeline && e.agentId !== undefined) {
      pipeline
        .dispatch({ type: 'turn-end', agentId: e.agentId, reason: e.reason, text: done.text })
        .catch(err => $.ui.log(`pipeline: turn end failed: ${String(err)}`))
    }
    return done
  })

  on('command.run', { command: 'pipeline' }, async $ => {
    await $.ui.open({ id: PANE, title: 'Pipeline' })
    return { text: 'Pipeline pane opened.' }
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Text } = $.ui.resolve(e)
    const { value = [] } = await $.state.get(rows)
    const width = Math.max(40, e.viewport?.columns ?? 100)
    const cut = (s: string): string => (s.length > width ? `${s.slice(0, width - 1)}~` : s)
    const slugWidth = Math.min(24, Math.max(4, ...value.map(t => t.slug.length)))
    return (
      <Box flexDirection="column">
        <Text bold>{cut(`${'task'.padEnd(slugWidth)}  ${'stage'.padEnd(10)}  ${'PR'.padEnd(5)}  ${'CI'.padEnd(8)}  ${'developer'.padEnd(11)}  rounds`)}</Text>
        {value.length === 0 && <Text dimColor>No tasks.</Text>}
        {value.map(t => (
          <Box flexDirection="column">
            <Text>
              {cut(
                `${t.slug.padEnd(slugWidth)}  ${t.stage.padEnd(10)}  ${(t.pr === 0 ? '-' : `#${t.pr}`).padEnd(5)}  ${t.ci.padEnd(8)}  ${t.developer.padEnd(11)}  ${t.rounds}`,
              )}
            </Text>
            <Text dimColor>{cut(`  ${t.line}`)}</Text>
          </Box>
        ))}
      </Box>
    )
  })
}
