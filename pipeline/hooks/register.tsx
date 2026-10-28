// Wires the pipeline to Claude Code: the tools of the liaison, the events of the
// subagents, the timer, the pane and the status line. Decisions are in
// machine.ts, the carrying out of them in driver.ts, the server and git in
// forge.ts.
import type { EngineInterface, On, Register } from 'claude-code'

import { type Config, CONFIG_PATH, checkLogs, parseConfig, parseSkills, type SkillRule } from './config'
import { createPipeline, type Input, type Pipeline, type Ports } from './driver'
import { makeForge } from './forge'
import { denyText, editedPaths, requiredSkills, skillName } from './guard'
import { DEVELOPERS, type Developer, POLL_MS, type State } from './model'
import { describeAll, summary, viewRows } from './view'

const PANE = 'pipeline'
const TOOL = (name: string): string => `mcp__pipeline__${name}`
const STORE_KEY = 'tasks'
const rows = { plugin: 'pipeline', key: 'tasks' } as const
/** Held by the engine for the session and kept over a reload of the module: absent at a new session. Written after a successful start. */
const started = { plugin: 'pipeline', key: 'started' } as const

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
  {
    name: 'pipeline_resume',
    description:
      'Continue the pipeline after a session start paused it: without "slug" every unfinished task (developers are told or replaced, reviewers started, checks polled, waiting tasks started, messages kept while paused delivered); with "slug" only that task, the others stay held. Call only after the user agreed to continue.',
    inputSchema: {
      type: 'object',
      properties: { slug: { type: 'string', description: 'Omit to resume every task' } },
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
  /** A new session (the pipeline pauses), not a reload of the module; read once at `session.start`. */
  fresh?: boolean
  /** The start in progress or done; cleared by a failed one so that the next call reads the file anew. */
  booting?: Promise<void>
}

/** One start at a time: every caller gets the same promise. */
function boot($: EngineInterface, rt: Runtime): Promise<void> {
  rt.booting ??= bootOnce($, rt).then(
    ok => {
      if (!ok) rt.booting = undefined
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
  await p.start({ fresh: rt.fresh ?? true })
  // Only a start that succeeded marks the session: a module that failed to start and is loaded again still finds the session new.
  await $.state.set(started, true).catch(err => $.ui.log(`pipeline: the session marker was not written: ${String(err)}`))
  $.clock.every(POLL_MS, () => {
    p.tick().catch(err => $.ui.log(`pipeline: tick failed: ${String(err)}`))
  })
  return true
}

// The skills a loop has loaded are kept in `$.state` (key `skills`, one value per loop: its agent id, or
// `main`) so that a reload of the mod keeps them. The references are written out where they are used:
// the engine wants literal plugin and key at the call.
const MAIN_LOOP = 'main'

async function loadedSkills($: EngineInterface, agentId: string | undefined): Promise<string[]> {
  return (await $.state.get({ plugin: 'pipeline', key: 'skills', id: agentId ?? MAIN_LOOP })).value ?? []
}

/** Adds the skill to the loop's set; a write that another beat is tried again on what stands. */
async function remember($: EngineInterface, agentId: string | undefined, skill: string): Promise<void> {
  for (let attempt = 0; attempt < 3; attempt++) {
    const ref = { plugin: 'pipeline', key: 'skills', id: agentId ?? MAIN_LOOP } as const
    const { value = [], version } = await $.state.get(ref)
    if (value.includes(skill) || (await $.state.set(ref, [...value, skill], { ifVersion: version })).isSet) return
  }
}

/** What the guard keeps in memory: the rules last read, the file they came from, the last problem logged. */
type GuardState = {
  /** The last good rules: a file that cannot be read or parsed leaves them as they were (none at first). */
  rules: SkillRule[]
  /** The modification time of the file `rules` were read from (or tried to be); the file is read again when it changes. */
  mtime?: number
  reported?: string
}

/** Logs a problem of the guard once. */
function reportGuard($: EngineInterface, g: GuardState, what: string): void {
  if (g.reported === what) return
  g.reported = what
  $.ui.log(`pipeline: skill guard: ${what}`)
}

async function readRules($: EngineInterface, g: GuardState): Promise<SkillRule[]> {
  const path = `${await $.session.root()}/${CONFIG_PATH}`
  const kept = g.rules.length === 0 ? 'edits are not guarded' : 'the rules read before stay'
  let mtime: number
  try {
    mtime = (await $.fs.stat(path)).mtimeMs
  } catch (err) {
    reportGuard($, g, `${CONFIG_PATH} could not be read: ${err instanceof Error ? err.message : String(err)}; ${kept} until it can be`)
    return g.rules
  }
  if (mtime === g.mtime) return g.rules
  let text: string
  try {
    text = await $.fs.read(path)
  } catch (err) {
    reportGuard($, g, `${CONFIG_PATH} could not be read: ${err instanceof Error ? err.message : String(err)}; ${kept} until it can be`)
    return g.rules
  }
  g.mtime = mtime
  const parsed = parseSkills(text)
  if (!parsed.ok) {
    reportGuard($, g, `${parsed.error}; ${kept} until the file is fixed`)
    return g.rules
  }
  g.reported = undefined
  return (g.rules = parsed.skills)
}

/** The rule of a skill that the call writes a file for and its loop has not loaded. */
async function missingSkill($: EngineInterface, g: GuardState, e: Record<string, unknown>): Promise<SkillRule | undefined> {
  const wanted = await readRules($, g)
  if (wanted.length === 0) return undefined
  const [cwd, root, home] = await Promise.all([$.session.cwd(), $.session.root(), $.env.get('HOME')])
  const needed = requiredSkills(editedPaths(str(e.tool), e, cwd, root, home), wanted)
  if (needed.length === 0) return undefined
  const loaded = await loadedSkills($, typeof e.agentId === 'string' ? e.agentId : undefined)
  return needed.find(r => !loaded.includes(r.skill))
}

/** Runs the work of the guard; a failure is logged once and the answer is undefined, so the edit goes on. */
async function guarded<T>($: EngineInterface, g: GuardState, what: string, work: () => Promise<T>): Promise<T | undefined> {
  try {
    return await work()
  } catch (err) {
    reportGuard($, g, `${what} failed: ${err instanceof Error ? err.message : String(err)}`)
    return undefined
  }
}

/**
 * The code-skill guard (guard.ts, config.md "skills"): an edit of a file that
 * a rule of the project names is denied until the loop has loaded the rule's
 * skill. It reads the `skills` key itself, so it works with no task running and
 * when the pipeline failed to start; a failure of its own never blocks an edit.
 */
function registerGuard(on: On): void {
  const g: GuardState = { rules: [] }

  on('tool.call', { tool: /^(Edit|Write|MultiEdit|NotebookEdit|Bash)$/ }, async ($, e, next) => {
    const missing = await guarded($, g, 'the check of an edit', () => missingSkill($, g, e as unknown as Record<string, unknown>))
    return missing === undefined ? next(e) : { deny: denyText(missing) }
  })

  on('tool.call', { tool: /^Skill$/ }, async ($, e, next) => {
    const done = await next(e)
    const name = str((e as unknown as Record<string, unknown>).skill)
    if (done.deny === undefined && done.isError !== true && name !== '') {
      await guarded($, g, 'noting a loaded skill', () => remember($, e.agentId, skillName(name)))
    }
    return done
  })

  // `/<skill>` typed by the person loads the skill into the main loop as the Skill tool does.
  on('command.run', async ($, e, next) => {
    const done = await next(e)
    await guarded($, g, 'noting a loaded skill', async () => {
      const skill = skillName(e.command)
      if ((await readRules($, g)).some(r => r.skill === skill)) await remember($, undefined, skill)
    })
    return done
  })

  // `/clear` ends the session while the process and the mod go on: the main loop starts with no skill loaded.
  on('session.end', async ($, e, next) => {
    if (e.reason === 'clear') {
      await guarded($, g, 'forgetting the skills at /clear', () => $.state.set({ plugin: 'pipeline', key: 'skills', id: MAIN_LOOP }, []))
    }
    return next(e)
  })

  // A compaction summarizes the skill text away: the loop loads it again. A `precompute` installs nothing.
  on('session.compact', async ($, e, next) => {
    const done = await next(e)
    if (done.skip === undefined && e.trigger !== 'precompute') {
      await guarded($, g, 'forgetting the skills at a compaction', () => $.state.set({ plugin: 'pipeline', key: 'skills', id: e.agentId ?? MAIN_LOOP }, []))
    }
    return done
  })
}

/**
 * Whether the agent ended its turn to wait for its own background work. The status of the agent decides when
 * it says `waiting` (work it owns) or that the agent is idle or ended; otherwise the background work SubagentStop
 * listed for it (see `register`) does.
 */
async function waitsForBackgroundWork($: EngineInterface, known: ReadonlySet<string>, agentId: string): Promise<boolean> {
  let status: string | undefined
  try {
    status = (await $.agent.list()).find(a => a.id === agentId)?.status
  } catch (err) {
    $.ui.log(`pipeline: the agent list could not be read: ${String(err)}`)
  }
  if (status === 'waiting') return true
  if (status !== undefined && status !== 'running' && status !== 'pending') return false
  return known.has(agentId)
}

export const register: Register = on => {
  const rt: Runtime = { root: '' }

  on('session.start', async ($, e, next) => {
    rt.root = await $.session.root()
    if (rt.fresh === undefined) {
      // `register` runs again and `session.start` fires again at a reload; the state of the engine survives it, a new session has none.
      rt.fresh = (await $.state.get(started)).value === undefined
    }
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

  served('pipeline_resume', (p, _c, a) => reply(p, { type: 'resume', slug: str(a.slug) || undefined }))

  // Claude Code ends the turn of a subagent that waits for its background commands and wakes it when they end:
  // such an end is no ending. The signs are the status `waiting` of the agent in `$.agent.list()` and the
  // background work in flight that the settings event SubagentStop lists. That list is the session's, so the
  // subagents in it (the other agents of the pipeline, this one too) say nothing about this agent.
  const backgroundWork = new Set<string>()

  on('classic.SubagentStop', async ($, e, next) => {
    if ((e.background_tasks ?? []).some(b => b.type !== 'subagent')) backgroundWork.add(e.agent_id)
    else backgroundWork.delete(e.agent_id)
    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    if (e.agentId !== undefined) backgroundWork.delete(e.agentId)
    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    const done = await next(e)
    if (rt.pipeline && e.agentId !== undefined) {
      const waiting = await waitsForBackgroundWork($, backgroundWork, e.agentId)
      rt.pipeline
        .dispatch({ type: 'turn-end', agentId: e.agentId, reason: e.reason, text: done.text, waiting })
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

  // Last: registered before the hooks above, a pass-through `tool.call` hook made `$.agent.spawn` lose the agent id.
  registerGuard(on)
}
