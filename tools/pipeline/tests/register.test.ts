// register.tsx through the engine: the tools, the events of the subagents and
// the pane. The server and git are not reached: process.run is answered here.
import { expect, mock, test } from 'claude-code/testing'

const TOOL = 'mcp__pipeline__pipeline_start'

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
  expect(registered).toEqual(['pipeline_start', 'pipeline_status', 'pipeline_send', 'pipeline_stop'])

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

  const status = await $.tool.call({ tool: 'mcp__pipeline__pipeline_status' } as never)
  expect(String(status.text ?? status.result)).toContain('feat:')
  expect(statuses.length).toBeGreaterThan(0)

  const refused = await $.tool.call({ tool: TOOL, slug: 'feat', developer: 'developer-1', brief: 'x' } as never)
  expect(refused.deny).toContain('exists')
})

test('a task from the store is woken, its turn end is read, the pane shows it', async ($, on) => {
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
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.root', async () => ({ value: '/repo' }))
  on('tool.register', async (_$, e) => ({ value: { tool: `mcp__pipeline__${e.name}` } }))
  on('command.register', async () => ({ value: { command: 'pipeline' } }))
  on('ui.status', async () => ({ value: undefined }))
  on('session.send', async (_$, e) => {
    sent.push(e.text)
    return { isDelivered: true }
  })
  on('turn.complete', async (_$, e) => ({ text: e.answer }))
  on('prompt.submit', async (_$, e) => {
    submitted.push(e.text)
    return { text: e.text }
  })

  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  // The agent of the earlier session is tried with send.
  expect(sent).toHaveLength(1)
  expect(sent[0]).toContain('restarted')

  // Its turn ends with a question: the liaison is told, the task waits for it.
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
