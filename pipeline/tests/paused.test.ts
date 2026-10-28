// A session start pauses the pipeline: it tells the lead what is unfinished and
// continues only on pipeline_resume (the register-level part is in register.test.ts).
import { describe, expect, test } from 'claude-code/testing'

import { createPipeline } from '../hooks/driver'
import { emptyState } from '../hooks/machine'
import type { Action, State, Task } from '../hooks/model'
import { describeAll, summary } from '../hooks/view'
import { fakes, Run, TEST_CONFIG } from './helpers'

const HEAD = 'a'.repeat(40)
const WAKING: Action['type'][] = ['spawn', 'tell', 'poll', 'salvage', 'read-review', 'merge', 'post-merge', 'rerun']

/** A task as an older version of the mod stored it: none of the new fields. */
function oldTask(slug: string, stage: Task['stage'], extra: Partial<Task> = {}): Task {
  return {
    slug,
    brief: `Brief of ${slug}.`,
    after: [],
    stage,
    starter: 'developer-1',
    developer: 'developer-1',
    agents: [],
    ci: 'none',
    rounds: { 'developer-1': 0, 'developer-2': 0, 'developer-3': 0 },
    nudged: false,
    unmergeable: 0,
    lastEvent: 'old',
    lastAt: 0,
    ...extra,
  }
}

const notes = (actions: Action[]): string[] => actions.flatMap(a => (a.type === 'notify' ? [a.text] : []))

/** Two tasks as a closed session left them: x develops, y waits for its check. */
function twoTasks(): Run {
  const r = new Run()
  r.begin('x', 'developer-2', 'x-d2')
  r.begin('y')
  r.ready('y', 8, HEAD)
  return r
}

const restore = (r: Run, agents: string[], extra: { orphans?: string[]; fresh?: boolean } = {}) =>
  r.feed({ type: 'restore', agents, orphans: extra.orphans ?? [], fresh: extra.fresh ?? true })

describe('a session start', () => {
  test('with live tasks pauses and does nothing but tell the lead', () => {
    const r = twoTasks()
    const res = restore(r, ['y-d1'], { orphans: ['/repo/.claude/worktrees/agent-zzz'] })
    expect(r.state.paused).toBe(true)
    expect(res.actions.filter(a => WAKING.includes(a.type))).toEqual([])
    expect(res.actions).toHaveLength(1)
    const [text] = notes(res.actions)
    expect(text).toContain('[pipeline] the session started; the pipeline is paused.')
    expect(text).toContain('- x: developing, no PR, a new developer-2 continues')
    expect(text).toContain('- y: checking, PR #8, CI is polled')
    expect(text).toContain('/repo/.claude/worktrees/agent-zzz')
    expect(text).toContain('Call pipeline_resume after the user agrees.')
    // The agents the store called running are not.
    expect(r.state.tasks.every(t => t.agents.every(a => !a.running))).toBe(true)
    expect(r.task('x').lost).toEqual(['x-d2'])
  })

  test('with none does not pause and tells nothing', () => {
    const r = new Run()
    r.state = { tasks: [oldTask('m', 'merged'), oldTask('b', 'blocked', { blockedReason: 'r' })], paused: true }
    const res = restore(r, [])
    expect(r.state.paused).toBe(false)
    expect(res.actions).toEqual([])
    expect(restore(new Run(), []).actions).toEqual([])
  })

  test('pauses whatever the stored flag says, and forgets the held flags', () => {
    const r = new Run()
    r.state = { tasks: [oldTask('a', 'checking', { pr: 3, head: HEAD, held: true })], paused: false }
    restore(r, [])
    expect(r.state.paused).toBe(true)
    expect(r.task('a').held).toBeUndefined()
  })

  test('an old store without the new fields is read', () => {
    const r = new Run()
    r.state = { tasks: [oldTask('a', 'developing', { agentId: 'a-d1', agents: [{ id: 'a-d1', role: 'developer', tier: 'developer-1', running: true }] }), oldTask('w', 'waiting')] }
    const res = restore(r, ['a-d1'])
    expect(r.state.paused).toBe(true)
    expect(notes(res.actions)[0]).toContain('- a: developing, no PR, developer-1 is told to continue')
    expect(notes(res.actions)[0]).toContain('- w: waiting, no PR, developer-1 starts')
    const resumed = r.feed({ type: 'resume' })
    expect(resumed.actions.find(a => a.type === 'tell')).toMatchObject({ agentId: 'a-d1', how: 'send' })
    expect(resumed.actions.find(a => a.type === 'spawn')).toMatchObject({ slug: 'w', role: 'developer' })
  })

  test('an old store without addenda or an install retry loads at a reload, takes a send, and reviews with the brief alone', () => {
    const r = new Run()
    const developer = { id: 'a-d1', role: 'developer' as const, tier: 'developer-1', running: false }
    r.state = { tasks: [oldTask('a', 'developing', { agentId: 'a-d1', agents: [developer] }), oldTask('c', 'checking', { pr: 3, head: HEAD })] }
    restore(r, ['a-d1'], { fresh: false })
    expect(r.state.installRetry).toBeUndefined()
    expect(r.feed({ type: 'tick' }).actions.some(a => a.type === 'install-retry')).toBe(false)

    const sent = r.feed({ type: 'send', slug: 'a', text: 'Also the edge case.' })
    expect(sent.reply).toContain('sent to developer-1')
    expect(r.task('a').addenda).toEqual(['Also the edge case.'])

    const res = r.poll('c', HEAD, 'success')
    const spawn = res.actions.find(a => a.type === 'spawn')
    expect(spawn?.type === 'spawn' && spawn.prompt).toContain('Brief of c.')
    expect(spawn?.type === 'spawn' && spawn.prompt).not.toContain('Messages of the lead')
  })

  test('a reload of the module in the same session does not pause: it polls and tells nobody who is alive', () => {
    const r = twoTasks()
    const res = restore(r, ['x-d2', 'y-d1'], { fresh: false })
    expect(r.state.paused).toBeUndefined()
    expect(res.actions.some(a => a.type === 'tell')).toBe(false)
    expect(res.actions.find(a => a.type === 'poll')).toMatchObject({ slug: 'y' })
    expect(notes(res.actions)).toEqual([])
  })

  test('a reload sends nothing to an agent the engine lists and keeps its running state; a dead one is replaced', () => {
    const r = twoTasks()
    r.begin('z')
    r.feed({ type: 'spawned', slug: 'y', agentId: 'y-rev', role: 'reviewer', tier: 'reviewer' })
    r.poll('y', HEAD, 'success')
    const res = restore(r, ['x-d2', 'y-d1', 'y-rev'], { fresh: false })
    expect(r.task('x').agents[0]?.running).toBe(true)
    expect(res.actions.some(a => a.type === 'tell' && (a.agentId === 'x-d2' || a.agentId === 'y-rev'))).toBe(false)
    // z-d1 is not listed: dead, salvaged as at resume.
    expect(res.actions.find(a => a.type === 'salvage')).toMatchObject({ slug: 'z', agentId: 'z-d1' })
    expect(r.task('z').agents[0]?.running).toBe(false)
  })

  test('a reload keeps a pause that is still on', () => {
    const r = twoTasks()
    restore(r, ['x-d2', 'y-d1'])
    const res = restore(r, ['x-d2', 'y-d1'], { fresh: false })
    expect(r.state.paused).toBe(true)
    expect(res.actions).toEqual([])
  })
})

describe('while the pipeline is paused', () => {
  test('tick does nothing', () => {
    const r = twoTasks()
    restore(r, [])
    r.now += 3_600_000
    expect(r.feed({ type: 'tick' }).actions).toEqual([])
  })

  test('pipeline_start records the task as waiting and starts nothing', () => {
    const r = twoTasks()
    restore(r, [])
    const res = r.feed({ type: 'start', slug: 'z', developer: 'developer-1', brief: 'Z', after: [] })
    expect(res.error).toBeUndefined()
    expect(res.reply).toContain('recorded; the pipeline is paused')
    expect(res.actions).toEqual([])
    expect(r.task('z').stage).toBe('waiting')
    r.now += 1_000
    expect(r.feed({ type: 'tick' }).actions).toEqual([])
    const resumed = r.feed({ type: 'resume' })
    expect(resumed.actions.find(a => a.type === 'spawn' && a.slug === 'z')).toBeDefined()
    expect(r.task('z').stage).toBe('developing')
  })

  test('pipeline_send records the text and says so; resume delivers it with the restart message', () => {
    const r = twoTasks()
    restore(r, ['x-d2', 'y-d1'])
    const res = r.feed({ type: 'send', slug: 'x', text: 'Use the second way.' })
    expect(res.error).toBeUndefined()
    expect(res.reply).toContain('the pipeline is paused')
    expect(res.actions).toEqual([])
    r.feed({ type: 'send', slug: 'x', text: 'And rename it.' })
    expect(r.task('x').heldSend).toBe('Use the second way.\n\nAnd rename it.')
    const resumed = r.feed({ type: 'resume' })
    const tell = resumed.actions.find(a => a.type === 'tell' && a.agentId === 'x-d2')
    expect(tell?.type === 'tell' && tell.text).toContain('restarted')
    expect(tell?.type === 'tell' && tell.text).toContain('Use the second way.\n\nAnd rename it.')
    expect(r.task('x').heldSend).toBeUndefined()
  })

  test('a send to a task that waits is added to its brief as before', () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'start', slug: 'w', developer: 'developer-1', brief: 'W', after: ['x'] })
    restore(r, [])
    const res = r.feed({ type: 'send', slug: 'w', text: 'More.' })
    expect(res.reply).toContain('added to the brief')
    expect(r.task('w').brief).toContain('More.')
  })

  test('a send to a task that waits for the lead goes back to the developer on resume', () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Which way?\nPIPELINE: question' })
    restore(r, ['x-d1'])
    r.feed({ type: 'send', slug: 'x', text: 'The first.' })
    expect(r.task('x').stage).toBe('lead')
    const resumed = r.feed({ type: 'resume' })
    expect(r.task('x').stage).toBe('developing')
    expect(resumed.actions.find(a => a.type === 'tell')).toMatchObject({ agentId: 'x-d1', text: 'The first.' })
  })

  test('a send to a blocked task is kept: its new developer-3 starts on resume', () => {
    const r = new Run()
    r.begin('x')
    r.begin('y')
    r.feed({ type: 'stop', slug: 'x', reason: 'r' })
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Pushed.' })
    restore(r, ['y-d1'])
    const res = r.feed({ type: 'send', slug: 'x', text: 'Go on.' })
    expect(res.actions).toEqual([])
    expect(r.task('x').stage).toBe('blocked')
    const resumed = r.feed({ type: 'resume' })
    expect(r.task('x').stage).toBe('developing')
    expect(resumed.actions.find(a => a.type === 'spawn')).toMatchObject({ slug: 'x', tier: 'developer-3' })
  })

  test('pipeline_stop works: it blocks the task, the kept message goes', () => {
    const r = twoTasks()
    restore(r, [])
    r.feed({ type: 'send', slug: 'x', text: 'Hint.' })
    const res = r.feed({ type: 'stop', slug: 'x', reason: 'enough' })
    expect(res.reply).toBe('stopped: x')
    expect(r.task('x').stage).toBe('blocked')
    expect(r.task('x').heldSend).toBeUndefined()
    expect(res.actions.some(a => a.type === 'tell')).toBe(false)
  })

  test('the turn end of an agent that is alive is kept and handled on resume', () => {
    const r = new Run()
    r.begin('x')
    r.begin('y')
    restore(r, ['x-d1', 'y-d1'])
    const end = r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Done.\nPIPELINE: pr=7' })
    expect(end.actions).toEqual([])
    expect(r.task('x').stage).toBe('developing')
    const resumed = r.feed({ type: 'resume' })
    expect(r.task('x').stage).toBe('checking')
    expect(resumed.actions.find(a => a.type === 'poll')).toMatchObject({ slug: 'x', pr: 7 })
    expect(resumed.actions.find(a => a.type === 'tell')).toMatchObject({ agentId: 'y-d1' })
    expect(r.task('x').deferred).toBeUndefined()
  })

  test('pipeline_status starts with the pause, the status line says paused', () => {
    const r = twoTasks()
    expect(summary(r.state)).toBe('1 developing · 1 checking')
    restore(r, [])
    const text = describeAll(r.state, TEST_CONFIG, r.now)
    expect(text.split('\n')[0]).toBe('PAUSED - pipeline_resume continues.')
    expect(text).toContain('x: developing held |')
    expect(summary(r.state)).toBe('paused · 1 developing · 1 checking')
    r.feed({ type: 'resume' })
    expect(describeAll(r.state, TEST_CONFIG, r.now)).not.toContain('PAUSED')
    expect(summary(r.state)).toBe('1 developing · 1 checking')
  })
})

describe('pipeline_resume', () => {
  test('without a slug unpauses and continues every task: the dead are salvaged, the others polled and told', () => {
    const r = twoTasks()
    restore(r, [])
    const res = r.feed({ type: 'resume' })
    expect(r.state.paused).toBe(false)
    expect(res.reply).toBe('resumed: x, y')
    expect(res.actions.find(a => a.type === 'salvage')).toMatchObject({ slug: 'x', agentId: 'x-d2' })
    expect(res.actions.find(a => a.type === 'poll')).toMatchObject({ slug: 'y' })
    r.now += 1_000
    expect(r.feed({ type: 'tick' }).actions.some(a => a.type === 'poll')).toBe(true)
  })

  test('when nothing is paused it says so and does nothing', () => {
    const r = twoTasks()
    const res = r.feed({ type: 'resume' })
    expect(res.reply).toContain('not paused')
    expect(res.actions).toEqual([])
  })

  test('with a slug resumes only that task; the others stay held, the pause is per task', () => {
    const r = twoTasks()
    r.begin('z')
    restore(r, ['x-d2', 'y-d1', 'z-d1'])
    const res = r.feed({ type: 'resume', slug: 'y' })
    expect(res.reply).toBe('y resumed; still held: x, z')
    expect(r.state.paused).toBe(false)
    expect(r.task('x').held).toBe(true)
    expect(r.task('y').held).toBeUndefined()
    expect(res.actions).toEqual([{ type: 'poll', slug: 'y', pr: 8 }])
    // The tick works for y only.
    r.now += 60_000
    const tick = r.feed({ type: 'tick' })
    expect(tick.actions).toEqual([{ type: 'poll', slug: 'y', pr: 8 }])
    // A send to a held task is kept; the status shows the hold.
    expect(r.feed({ type: 'send', slug: 'x', text: 'Hint.' }).reply).toContain('paused')
    const text = describeAll(r.state, TEST_CONFIG, r.now)
    expect(text.split('\n')[0]).toBe('HELD: x, z - pipeline_resume { slug } continues.')
    expect(summary(r.state)).toBe('2 held · 2 developing · 1 checking')
    // Then the rest.
    const rest = r.feed({ type: 'resume' })
    expect(rest.reply).toBe('resumed: x, z')
    expect(rest.actions.filter(a => a.type === 'tell').map(a => a.type === 'tell' && a.agentId).sort()).toEqual(['x-d2', 'z-d1'])
    expect(r.state.tasks.some(t => t.held)).toBe(false)
  })

  test('a task finished by another task does not rebase a held one', () => {
    const r = twoTasks()
    restore(r, ['x-d2', 'y-d1'])
    r.feed({ type: 'resume', slug: 'x' })
    const res = r.feed({ type: 'post-merge', slug: 'x', pull: 'ok', install: 'ok', note: '', leftovers: [], conflicted: ['y'] })
    expect(res.actions.some(a => a.type === 'tell' && a.agentId === 'y-d1')).toBe(false)
    expect(r.task('y').stage).toBe('checking')
  })

  test('an unknown slug is an error; a task that is not held is left alone', () => {
    const r = twoTasks()
    restore(r, [])
    expect(r.feed({ type: 'resume', slug: 'nope' }).error).toContain('no task "nope"')
    r.feed({ type: 'resume' })
    const res = r.feed({ type: 'resume', slug: 'x' })
    expect(res.reply).toContain('not held')
    expect(res.actions).toEqual([])
  })
})

describe('through the driver', () => {
  test('a restart spawns nothing until pipeline_resume; a start while paused waits for it', async () => {
    const f = fakes()
    const p = createPipeline(f.ports)
    await p.start()
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    expect(f.spawned).toHaveLength(1)
    f.gone.add(f.spawned[0]?.agentId ?? '')

    const q = createPipeline(f.ports)
    await q.start()
    await q.settle()
    expect(q.state().paused).toBe(true)
    expect(f.spawned).toHaveLength(1)
    expect(f.sent).toEqual([])
    expect(f.notes).toHaveLength(1)
    expect(f.notes[0]).toContain('the pipeline is paused')
    await q.dispatch({ type: 'start', slug: 'other', developer: 'developer-1', brief: 'O', after: [] })
    await q.tick()
    await q.settle()
    expect(f.spawned).toHaveLength(1)

    await q.dispatch({ type: 'resume' })
    await q.settle()
    expect(f.spawned).toHaveLength(3)
    expect(q.state().paused).toBe(false)
  })

  test('a reload of the module (not fresh) sends a living developer nothing', async () => {
    const f = fakes()
    const p = createPipeline(f.ports)
    await p.start()
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    const q = createPipeline(f.ports)
    await q.start({ fresh: false })
    await q.settle()
    expect(f.sent).toEqual([])
    expect(f.appended).toEqual([])
    expect(f.spawned).toHaveLength(1)
    expect(q.state().paused).toBe(false)
  })
})

test('an empty state is not paused', () => {
  expect(emptyState().paused).toBeUndefined()
  const state: State = { tasks: [] }
  expect(summary(state)).toBeUndefined()
})
