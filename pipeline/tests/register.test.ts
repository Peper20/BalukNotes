// register.tsx through the engine: the tools, the events of the subagents and
// the pane. The server and git are not reached: process.run is answered here.
import type { On } from 'claude-code'
import { type Engine, describe, expect, mock, test } from 'claude-code/testing'

const TOOL = 'mcp__pipeline__pipeline_start'
const CONFIG = JSON.stringify({
  project: 'widgets',
  forge: { url: 'http://forge.test', repo: 'acme/widgets' },
  targetedTests: 'make test-one',
})

test('the tools are registered and pipeline_start starts a developer', async ($, on) => {
  mock.clock(on)
  mock.store(on)
  const registered: string[] = []
  const spawned: { prompt: string }[] = []
  const statuses: (string | undefined)[] = []
  const processes: string[] = []
  const submitted: string[] = []
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.root', async () => ({ value: '/repo' }))
  on('fs.read', async () => ({ value: CONFIG }))
  on('tool.register', async (_$, e) => {
    registered.push(e.name)
    return { value: { tool: `mcp__pipeline__${e.name}` } }
  })
  on('command.register', async () => ({ value: { command: 'pipeline' } }))
  on('ui.status', async (_$, e) => {
    statuses.push(e.text)
    return { value: undefined }
  })
  on('agent.spawn', async (_$, e) => {
    spawned.push({ prompt: e.prompt })
    return { model: 'm', agentId: 'agent-1' }
  })
  on('agent.list', async () => ({ value: [] }))
  on('process.run', async (_$, e) => {
    processes.push(e.argv.join(' '))
    return { value: { exitCode: 0, stdout: '', stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('turn.complete', async (_$, e) => ({ text: e.answer }))
  on('prompt.submit', async (_$, e) => {
    submitted.push(e.text)
    return { text: e.text }
  })

  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  expect(registered).toEqual(['pipeline_start', 'pipeline_status', 'pipeline_send', 'pipeline_stop', 'pipeline_resume'])

  const started = await $.tool.call({
    tool: TOOL,
    slug: 'feat',
    developer: 'developer-2',
    brief: 'Do the thing.',
    after: [],
  } as never)
  expect(started.deny).toBeUndefined()
  expect(String(started.text ?? started.result)).toContain('developer-2')
  expect(spawned).toHaveLength(1)
  expect(spawned[0]?.prompt).toContain('Do the thing.')
  expect(spawned[0]?.prompt).toContain('task/feat')
  expect(spawned[0]?.prompt).toContain('make test-one')
  expect(spawned[0]?.prompt).toContain('Project: widgets')

  const status = await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)
  expect(String(status.text ?? status.result)).toContain('feat:')
  expect(statuses.length).toBeGreaterThan(0)
  expect(statuses[statuses.length - 1]).toBe('1 developing')

  const refused = await $.tool.call({ tool: TOOL, slug: 'feat', developer: 'developer-1', brief: 'x' } as never)
  expect(refused.deny).toContain('exists')
})

test('a task from the store waits for pipeline_resume, then is woken; its turn end is read, the pane shows it', async ($, on) => {
  mock.clock(on)
  const rounds = { 'developer-1': 0, 'developer-2': 0, 'developer-3': 0 }
  mock.store(on, {
    tasks: {
      tasks: [
        {
          slug: 'feat',
          brief: 'B',
          after: [],
          stage: 'developing',
          starter: 'developer-2',
          developer: 'developer-2',
          agentId: 'agent-1',
          agents: [{ id: 'agent-1', role: 'developer', tier: 'developer-2', running: true }],
          ci: 'none',
          rounds,
          nudged: false,
          unmergeable: 0,
          lastEvent: 'started',
          lastAt: 0,
        },
      ],
    },
  })
  const sent: string[] = []
  const submitted: string[] = []
  const statuses: (string | undefined)[] = []
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.root', async () => ({ value: '/repo' }))
  on('fs.read', async () => ({ value: CONFIG }))
  on('tool.register', async (_$, e) => ({ value: { tool: `mcp__pipeline__${e.name}` } }))
  on('command.register', async () => ({ value: { command: 'pipeline' } }))
  on('ui.status', async (_$, e) => {
    statuses.push(e.text)
    return { value: undefined }
  })
  on('agent.list', async () => ({ value: [{ id: 'agent-1', description: 'feat', type: 'developer-2', status: 'running' }] }))
  on('session.send', async (_$, e) => {
    sent.push(e.text)
    return { isDelivered: true }
  })
  on('turn.complete', async (_$, e) => ({ text: e.answer }))
  on('prompt.submit', async (_$, e) => {
    submitted.push(e.text)
    return { text: e.text }
  })

  on('ui.log', async () => ({ value: undefined }))

  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  // A new session: nothing is woken, the lead is told what is unfinished.
  expect(sent).toHaveLength(0)
  const status = String((await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)).result)
  expect(status.startsWith('PAUSED - pipeline_resume continues.')).toBe(true)
  expect(submitted.join('|')).toContain('[pipeline] the session started; the pipeline is paused.')
  expect(submitted.join('|')).toContain('feat: developing, no PR, developer-2 is told to continue')
  expect(statuses[statuses.length - 1]).toBe('paused · 1 developing')

  // pipeline_resume: the agent of the earlier session is tried with send.
  const resumed = await $.tool.call({ tool: 'mcp__pipeline__pipeline_resume' } as never)
  expect(String(resumed.text ?? resumed.result)).toContain('resumed: feat')
  expect(sent).toHaveLength(1)
  expect(sent[0]).toContain('restarted')
  expect(statuses[statuses.length - 1]).toBe('1 developing')

  // Its turn ends with a question: the lead is told, the task waits for it.
  await $.turn.complete({ answer: 'Which way?\nPIPELINE: question', durationMs: 1, isAborted: false, turnId: 't1', agentId: 'agent-1', reason: 'answer' })
  // pipeline_send goes through the same queue, so its answer comes after the turn end was handled.
  const reply = await $.tool.call({ tool: 'mcp__pipeline__pipeline_send', slug: 'feat', text: 'Take the first.' } as never)
  expect(String(reply.text ?? reply.result)).toContain('sent to developer-2')
  expect(sent[sent.length - 1]).toBe('Take the first.')
  for (let i = 0; i < 5; i++) await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)
  expect(submitted.join('|')).toContain('[pipeline] feat (no pull request): the developer has a question')

  // The pane draws the tasks.
  const pane = await $.ui.mount({ plugin: 'pipeline', surface: 'terminal', component: 'Pane', props: {} as never, requestId: 'pipeline' })
  expect((await pane.find({ type: 'Text', text: /feat/ }))?.text).toContain('developer-2')
  await pane.unmount()
})

/** A session whose project layer is whatever `config.file` holds (undefined: the file cannot be read). */
async function configSession($: Engine, on: On, config: { file?: string; reads?: number }) {
  mock.clock(on)
  mock.store(on)
  const submitted: string[] = []
  const spawned: string[] = []
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.root', async () => ({ value: '/repo' }))
  on('fs.read', async () => {
    config.reads = (config.reads ?? 0) + 1
    if (config.file === undefined) throw new Error('no such file')
    return { value: config.file }
  })
  on('tool.register', async (_$, e) => ({ value: { tool: `mcp__pipeline__${e.name}` } }))
  on('command.register', async () => ({ value: { command: 'pipeline' } }))
  on('ui.status', async () => ({ value: undefined }))
  on('ui.log', async () => ({ value: undefined }))
  on('agent.spawn', async (_$, e) => {
    spawned.push(e.prompt)
    return { model: 'm', agentId: 'agent-1' }
  })
  on('agent.list', async () => ({ value: [] }))
  on('process.run', async () => ({ value: { exitCode: 0, stdout: '', stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }))
  on('turn.complete', async (_$, e) => ({ text: e.answer }))
  on('prompt.submit', async (_$, e) => {
    submitted.push(e.text)
    return { text: e.text }
  })
  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  return { submitted, spawned }
}

const START = { tool: TOOL, slug: 'feat', developer: 'developer-1', brief: 'x' } as never
/** The announcements are not awaited by the mod: let them through. */
const flush = async ($: Engine): Promise<void> => {
  for (let i = 0; i < 5; i++) await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)
}

test('a config file that cannot be read: reported once, no task starts', async ($, on) => {
  const { submitted, spawned } = await configSession($, on, {})
  await flush($)
  expect(submitted).toHaveLength(1)
  expect(submitted[0]).toContain('.claude/pipeline.json could not be read')
  const refused = await $.tool.call(START)
  expect(refused.deny).toContain('the pipeline is not started')
  expect(refused.deny).toContain('could not be read')
  expect(spawned).toHaveLength(0)
  await flush($)
  expect(submitted).toHaveLength(1)
})

test('an invalid config: reported once, no task starts; the fixed file is read on the next call', async ($, on) => {
  const config = { file: '{ "forge": { "url": "nope" } }' }
  const { submitted, spawned } = await configSession($, on, config)
  await flush($)
  expect(submitted).toHaveLength(1)
  expect(submitted[0]).toContain('"forge.url" must be an http(s) URL')
  expect((await $.tool.call(START)).deny).toContain('"forge.url" must be an http(s) URL')
  expect(spawned).toHaveLength(0)
  expect(submitted).toHaveLength(1)

  config.file = CONFIG
  const started = await $.tool.call(START)
  expect(started.deny).toBeUndefined()
  expect(spawned).toHaveLength(1)
})

test('boot runs once: calls that come together share one start; a config error lets the next call read the file anew', async ($, on) => {
  const config: { file?: string; reads?: number } = { file: '{ "forge": { "url": "nope" } }' }
  const { spawned } = await configSession($, on, config)
  expect(config.reads).toBe(1)
  // Two calls at once re-read the file once between them.
  await Promise.all([$.tool.call(START), $.tool.call(START)])
  expect(config.reads).toBe(2)
  config.file = CONFIG
  const [a, b] = await Promise.all([$.tool.call(START), $.tool.call({ ...(START as object), slug: 'other' } as never)])
  expect(config.reads).toBe(3)
  expect(a.deny).toBeUndefined()
  expect(b.deny).toBeUndefined()
  expect(spawned).toHaveLength(2)
  // Started: no more reads.
  await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)
  expect(config.reads).toBe(3)
})

test('pipeline_status: without all the merged tasks are cut to three, with all they are shown', async ($, on) => {
  mock.clock(on)
  const rounds = { 'developer-1': 0, 'developer-2': 0, 'developer-3': 0 }
  const merged = (slug: string, at: number) => ({
    slug,
    brief: 'B',
    after: [],
    stage: 'merged',
    starter: 'developer-1',
    developer: 'developer-1',
    agents: [],
    ci: 'none',
    rounds,
    nudged: false,
    unmergeable: 0,
    lastEvent: 'merged and cleaned up',
    lastAt: at,
    mergedAt: at,
  })
  mock.store(on, { tasks: { tasks: [1, 2, 3, 4, 5].map(i => merged(`m${i}`, i)) } })
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.root', async () => ({ value: '/repo' }))
  on('fs.read', async () => ({ value: CONFIG }))
  on('tool.register', async (_$, e) => ({ value: { tool: `mcp__pipeline__${e.name}` } }))
  on('command.register', async () => ({ value: { command: 'pipeline' } }))
  on('ui.status', async () => ({ value: undefined }))
  on('agent.list', async () => ({ value: [] }))
  on('process.run', async () => ({ value: { exitCode: 0, stdout: '', stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }))
  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  const short = String((await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)).result)
  expect(short).toContain('m5:')
  expect(short).not.toContain('m1:')
  expect(short).toContain('2 older merged tasks: pipeline_status { all: true }')
  const all = String((await $.tool.call({ tool: 'mcp__pipeline__pipeline_status', all: true } as never)).result)
  expect(all).toContain('m1:')
  expect(all).not.toContain('older merged')
})

/**
 * A session over a stored task; `reload`: the engine already holds what an earlier load of the module wrote.
 * `config.file` is the project layer, read on every try of the start; `config.status`, the agent's in `$.agent.list()`.
 */
async function storedSession($: Engine, on: On, stage: 'developing' | 'merged', reload: boolean, config: { file: string; status?: string } = { file: CONFIG }) {
  mock.clock(on)
  const rounds = { 'developer-1': 0, 'developer-2': 0, 'developer-3': 0 }
  mock.store(on, {
    tasks: {
      tasks: [
        {
          slug: 'feat',
          brief: 'B',
          after: [],
          stage,
          starter: 'developer-2',
          developer: 'developer-2',
          agentId: 'agent-1',
          agents: [{ id: 'agent-1', role: 'developer', tier: 'developer-2', running: true }],
          ci: 'none',
          rounds,
          nudged: false,
          unmergeable: 0,
          lastEvent: 'started',
          lastAt: 0,
        },
      ],
      paused: false,
    },
  })
  const sent: string[] = []
  const submitted: string[] = []
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.root', async () => ({ value: '/repo' }))
  on('fs.read', async () => ({ value: config.file }))
  on('tool.register', async (_$, e) => ({ value: { tool: `mcp__pipeline__${e.name}` } }))
  on('command.register', async () => ({ value: { command: 'pipeline' } }))
  on('ui.status', async () => ({ value: undefined }))
  on('ui.log', async () => ({ value: undefined }))
  on('agent.list', async () => ({ value: [{ id: 'agent-1', description: 'feat', type: 'developer-2', status: (config.status ?? 'running') as never }] }))
  on('process.run', async () => ({ value: { exitCode: 0, stdout: '', stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }))
  on('turn.complete', async (_$, e) => ({ text: e.answer }))
  on('classic.SubagentStop', async () => ({}))
  on('session.send', async (_$, e) => {
    sent.push(e.text)
    return { isDelivered: true }
  })
  on('prompt.submit', async (_$, e) => {
    submitted.push(e.text)
    return { text: e.text }
  })
  const marked: string[] = []
  on('state.get', async (_$, e, next) => (e.key === 'started' && reload ? { value: { value: true, version: 1 } } : next(e)))
  on('state.set', async (_$, e, next) => {
    marked.push(e.key)
    return next(e)
  })
  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  await flush($)
  return { sent, submitted, marked }
}

test('a new session pauses even a store that says it is not paused; the state marker is then set', async ($, on) => {
  const { sent, submitted, marked } = await storedSession($, on, 'developing', false)
  expect(sent).toEqual([])
  expect(submitted.join('|')).toContain('the pipeline is paused')
  expect(marked).toContain('started')
})

test('a reload of the module in the same session does not pause and sends a living developer nothing', async ($, on) => {
  const { sent, submitted } = await storedSession($, on, 'developing', true)
  expect(sent).toEqual([])
  expect(submitted.join('|')).not.toContain('paused')
  const status = String((await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)).result)
  expect(status).not.toContain('PAUSED')
})

test('a new session with no unfinished task is not paused and tells nothing', async ($, on) => {
  const { sent, submitted } = await storedSession($, on, 'merged', false)
  expect(sent).toEqual([])
  expect(submitted).toEqual([])
  const status = String((await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)).result)
  expect(status).not.toContain('PAUSED')
})

test('a new session whose config is broken writes no session marker: a later load still finds the session new and pauses', async ($, on) => {
  const config = { file: '{ "forge": { "url": "nope" } }' }
  const { sent, submitted, marked } = await storedSession($, on, 'developing', false, config)
  // A load of the module after the fix reads the marker: absent, so its start is that of a new session.
  expect(marked).not.toContain('started')
  expect(submitted.join('|')).not.toContain('the pipeline is paused')

  config.file = CONFIG
  const status = String((await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)).result)
  expect(status.startsWith('PAUSED')).toBe(true)
  await flush($)
  expect(sent).toEqual([])
  expect(marked).toContain('started')
})

describe('an agent that waits for its own background work is not asked to finish', () => {
  const end = ($: Engine) => $.turn.complete({ answer: 'Measuring; waiting for the run.', durationMs: 1, isAborted: false, turnId: 't1', agentId: 'agent-1', reason: 'answer' })
  const stop = ($: Engine, background: { id: string; type: string; status: string; description: string }[]) =>
    $.classic.SubagentStop({ stop_hook_active: false, agent_id: 'agent-1', agent_transcript_path: '/t', agent_type: 'developer-2', background_tasks: background })
  const NUDGE = /PIPELINE/

  test('SubagentStop lists background work in flight: no nudge', async ($, on) => {
    const { sent } = await storedSession($, on, 'developing', true)
    await stop($, [{ id: 'b1', type: 'shell', status: 'running', description: 'a measurement' }])
    await end($)
    await flush($)
    expect(sent.filter(t => NUDGE.test(t))).toEqual([])
  })

  test('the agent is waiting in the agent list: no nudge', async ($, on) => {
    const { sent } = await storedSession($, on, 'developing', true, { file: CONFIG, status: 'waiting' })
    await end($)
    await flush($)
    expect(sent.filter(t => NUDGE.test(t))).toEqual([])
  })

  test('an agent the list calls idle is not waiting, whatever SubagentStop listed', async ($, on) => {
    const idle = await storedSession($, on, 'developing', true, { file: CONFIG, status: 'idle' })
    await stop($, [{ id: 'b1', type: 'shell', status: 'running', description: 'another agent\'s build' }])
    await end($)
    await flush($)
    expect(idle.sent.filter(t => NUDGE.test(t))).toHaveLength(1)
  })

  test('background work that is only subagents says nothing about this agent', async ($, on) => {
    const { sent } = await storedSession($, on, 'developing', true)
    await stop($, [{ id: 'a2', type: 'subagent', status: 'running', description: 'another developer' }])
    await end($)
    await flush($)
    expect(sent.filter(t => NUDGE.test(t))).toHaveLength(1)
  })

  test('neither sign: the missing line is asked for', async ($, on) => {
    const { sent } = await storedSession($, on, 'developing', true)
    await stop($, [])
    await end($)
    await flush($)
    expect(sent.filter(t => NUDGE.test(t))).toHaveLength(1)
  })
})
