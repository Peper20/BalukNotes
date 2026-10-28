// Wires the pipeline to Claude Code: the tools of the liaison, the events of the
// subagents, the timer, the pane and the status line. Decisions are in
// machine.ts, the carrying out of them in driver.ts, the server and git in
// forge.ts.
import type { EngineInterface, Register } from 'claude-code'

import { type Config, CONFIG_PATH, checkLogs, parseConfig } from './config'
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
    description:
      'The tasks of the pipeline: stage, pull request, head, CI state, developer and agent id, rounds, last event. Without "all": every task that is not merged and the last 3 merged ones.',
    inputSchema: {
      type: 'object',
      properties: { all: { type: 'boolean', description: 'Also every older merged task still kept (14 days after the merge)' } },
    },
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

/** Why the project layer is unusable, or the config. */
async function loadConfig($: EngineInterface, root: string): Promise<{ config: Config } | { error: string }> {
  let text: string
  try {
    text = await $.fs.read(`${root}/${CONFIG_PATH}`)
  } catch (err) {
    return { error: `${CONFIG_PATH} could not be read: ${err instanceof Error ? err.message : String(err)}` }
  }
  const parsed = parseConfig(text)
  return parsed.ok ? { config: parsed.config } : { error: parsed.error }
}

/** What the registration keeps between events: the pipeline once it started, the project layer, the state of the start. */
type Runtime = {
  pipeline?: Pipeline
  config?: Config
  root: string
  /** The problem with the project layer that the last start found; the mod starts no task while it stands. */
  configError?: string
  reportedError?: string
  /** The start in progress or done; cleared by a failed one so that the next call reads the file anew. */
  booting?: Promise<void>
}

/** One start at a time: every caller gets the same promise. */
function boot($: EngineInterface, rt: Runtime): Promise<void> {
  rt.booting ??= bootOnce($, rt).then(
    started => {
      if (!started) rt.booting = undefined
    },
    err => {
      rt.booting = undefined
      throw err
    },
  )
  return rt.booting
}

/** Reads the project layer and starts the pipeline; on a bad file it reports once, starts nothing and answers false. */
async function bootOnce($: EngineInterface, rt: Runtime): Promise<boolean> {
  const loaded = await loadConfig($, rt.root)
  if ('error' in loaded) {
    rt.configError = loaded.error
    if (rt.reportedError !== loaded.error) {
      rt.reportedError = loaded.error
      $.ui.log(`pipeline: ${loaded.error}`)
      void $.prompt
        .submit({ text: `[pipeline] ${loaded.error}. The pipeline starts no task until the file is fixed (see config.md of the kit); call a pipeline tool again to read it anew.` })
        .catch(() => undefined)
    }
    return false
  }
  rt.configError = undefined
  rt.config = loaded.config
  const ports: Ports = {
    config: loaded.config,
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
    agentIds: async () => (await $.agent.list()).map(a => a.id),
    forge: makeForge((argv, init) => $.process.run(argv, init), {
      root: rt.root,
      sleep: ms => $.clock.sleep(ms),
      write: (path, text) => $.fs.write(path, text),
      afterMerge: loaded.config.afterMerge,
      checkLogs: checkLogs(loaded.config),
    }),
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
  rt.pipeline = p
  await p.start()
  $.clock.every(POLL_MS, () => {
    p.tick().catch(err => $.ui.log(`pipeline: tick failed: ${String(err)}`))
  })
  return true
}

export const register: Register = on => {
  const rt: Runtime = { root: '' }

  on('session.start', async ($, e, next) => {
    rt.root = await $.session.root()
    for (const t of TOOLS) await $.tool.register({ ...t, isDeferred: false })
    await $.command.register({ name: 'pipeline', description: 'Show the tasks of the development pipeline' })
    await boot($, rt)
    return next(e)
  })

  const served = (
    name: string,
    handle: (p: Pipeline, c: Config, args: Record<string, unknown>, now: number) => Promise<string>,
  ): void => {
    on('tool.call', { tool: new RegExp(`^${TOOL(name)}$`) }, async ($, e) => {
      if (e.agentId !== undefined) return { deny: 'the pipeline tools are for the liaison, not for subagents' }
      if (rt.booting !== undefined || rt.configError !== undefined) await boot($, rt)
      const { pipeline, config } = rt
      if (!pipeline || !config) {
        return { deny: rt.configError === undefined ? 'the pipeline is not started yet' : `the pipeline is not started: ${rt.configError}` }
      }
      try {
        return { result: await handle(pipeline, config, e as unknown as Record<string, unknown>, await $.clock.now()) }
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

  served('pipeline_start', (p, _c, a) =>
    reply(p, {
      type: 'start',
      slug: str(a.slug),
      developer: str(a.developer) as Developer,
      brief: str(a.brief),
      after: Array.isArray(a.after) ? a.after.map(str).filter(s => s !== '') : [],
    }),
  )
  served('pipeline_status', async (p, c, a, now) => describeAll(p.state(), c, now, a.all === true))
  served('pipeline_send', (p, _c, a) => reply(p, { type: 'send', slug: str(a.slug), text: str(a.text) }))
  served('pipeline_stop', (p, _c, a) =>
    reply(p, { type: 'stop', slug: str(a.slug) || undefined, reason: str(a.reason) || 'no reason given' }),
  )

  on('turn.complete', async ($, e, next) => {
    const done = await next(e)
    if (rt.pipeline && e.agentId !== undefined) {
      rt.pipeline
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
