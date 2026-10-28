// The five failures the mod is robust against: a check that did not finish, a dead
// developer's worktree, an unbounded task list, a send lost at the end of a turn,
// a boot run twice (that one is in register.test.ts).
import { describe, expect, test } from 'claude-code/testing'

import { createPipeline } from '../hooks/driver'
import { emptyState, step } from '../hooks/machine'
import { CLOSED_POLL_MS, KEEP_MERGED_MS, RERUN_GRACE_MS, type State, type Task } from '../hooks/model'
import { describeAll } from '../hooks/view'
import { fakes, Run, TEST_CONFIG } from './helpers'

const HEAD = 'a'.repeat(40)
const HEAD2 = 'b'.repeat(40)
const DAY = 24 * 60 * 60 * 1000

/** A task as an older version of the mod stored it: none of the new fields. */
function oldTask(slug: string, stage: Task['stage'], lastAt: number, extra: Partial<Task> = {}): Task {
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
    lastAt,
    ...extra,
  }
}

/** A run whose state is `state`, as if the store held it. */
function runOf(state: State): Run {
  const r = new Run()
  r.state = state
  return r
}

describe('a check that did not finish', () => {
  test('is re-run, costs no round, and is counted per head', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    const res = r.poll('x', HEAD, 'failure', 'cancelled', false)
    expect(res.actions).toEqual([{ type: 'rerun', slug: 'x', pr: 7, head: HEAD }])
    expect(r.task('x').rounds['developer-1']).toBe(0)
    expect(r.task('x').stage).toBe('checking')
    expect(r.task('x').reruns).toBe(1)
    r.now += RERUN_GRACE_MS
    expect(r.poll('x', HEAD, 'error', 'cancelled', false).actions).toHaveLength(1)
    expect(r.task('x').reruns).toBe(2)
    // A new head starts the count again.
    r.poll('x', HEAD2, 'error', 'cancelled', false)
    expect(r.task('x').reruns).toBe(1)
    expect(r.task('x').rounds['developer-1']).toBe(0)
  })

  test('the third time goes to the liaison: no round, no block', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'failure', 'job cancelled', false)
    r.now += RERUN_GRACE_MS
    r.poll('x', HEAD, 'failure', 'job cancelled', false)
    r.now += RERUN_GRACE_MS
    const res = r.poll('x', HEAD, 'failure', 'job cancelled', false)
    expect(r.task('x').stage).toBe('liaison')
    expect(r.task('x').rounds['developer-1']).toBe(0)
    expect(res.actions.filter(a => a.type === 'rerun')).toHaveLength(0)
    const notice = res.actions.find(a => a.type === 'notify')
    expect(notice?.type === 'notify' && notice.text).toContain(`the check of ${HEAD.slice(0, 7)} did not finish 3 times: job cancelled`)
  })

  test('the liaison resuming the task from the liaison stage starts the count again', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    for (let i = 0; i < 3; i++) {
      r.poll('x', HEAD, 'failure', 'job cancelled', false)
      r.now += RERUN_GRACE_MS
    }
    expect(r.task('x').stage).toBe('liaison')
    expect(r.task('x').reruns).toBe(2)
    r.feed({ type: 'send', slug: 'x', text: 'Push an empty commit.' })
    expect(r.task('x').reruns).toBe(0)
    expect(r.task('x').rerunAt).toBeUndefined()
    // The same head is polled again at once and is re-run.
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Pushed.\nPIPELINE: pr=7' })
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'open', head: HEAD, ci: 'failure', mergeable: true, detail: 'cancelled', finished: false })
    expect(r.task('x').reruns).toBe(1)
  })

  test('a poll soon after a re-run that still sees the unfinished failure is pending and uses up no try', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'failure', 'cancelled', false)
    expect(r.task('x').rerunAt).toBe(r.now)
    for (let i = 0; i < 2; i++) {
      r.now += 60_000
      const res = r.poll('x', HEAD, 'failure', 'cancelled', false)
      expect(res.actions).toEqual([])
      expect(r.task('x').reruns).toBe(1)
      expect(r.task('x').ci).toBe('pending')
      expect(r.task('x').stage).toBe('checking')
    }
    // The re-run's own run was cancelled too, after the wait: that is the next try.
    r.now += 60_000
    expect(r.poll('x', HEAD, 'failure', 'cancelled', false).actions).toEqual([{ type: 'rerun', slug: 'x', pr: 7, head: HEAD }])
    expect(r.task('x').reruns).toBe(2)
    // A new head forgets the time.
    r.poll('x', HEAD2, 'pending')
    expect(r.task('x').rerunAt).toBeUndefined()
  })

  test('a failure the check commented is a round as before', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    const res = r.poll('x', HEAD, 'failure', 'clippy', true)
    expect(res.actions.some(a => a.type === 'rerun')).toBe(false)
    expect(r.task('x').rounds['developer-1']).toBe(1)
  })

  test('a failed re-run is noted and uses up the try', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'failure', 'cancelled', false)
    r.feed({ type: 'rerun-failed', slug: 'x', head: HEAD, reason: 'run 3 is running' })
    expect(r.task('x').lastEvent).toContain('re-run')
    expect(r.task('x').lastEvent).toContain('run 3 is running')
    expect(r.task('x').reruns).toBe(1)
  })

  test('the driver re-runs the check of the head and reads the status again', async () => {
    const f = fakes()
    const p = createPipeline(f.ports)
    await p.start()
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    f.server.open(7, HEAD)
    f.server.setStatus(HEAD, 'failure', 'cancelled', false)
    await p.dispatch({ type: 'turn-end', agentId: f.spawned[0]?.agentId ?? '', reason: 'answer', text: 'PIPELINE: pr=7' })
    await p.settle()
    expect(f.server.calls).toContain(`rerun ${HEAD}`)
    expect(p.state().tasks[0]?.rounds['developer-1']).toBe(0)
    f.server.rerunThrows = 'no actions run'
    f.time.now += RERUN_GRACE_MS
    await p.tick()
    await p.settle()
    expect(p.state().tasks[0]?.lastEvent).toContain('no actions run')
  })
})

describe('a dead developer', () => {
  test('restore: a running developer the engine does not list is salvaged, then replaced', async () => {
    const r = new Run()
    r.begin('x', 'developer-2', 'x-d2')
    const res = r.restart([])
    expect(res.actions.some(a => a.type === 'tell' || a.type === 'spawn')).toBe(false)
    expect(res.actions.find(a => a.type === 'salvage')).toMatchObject({ slug: 'x', agentId: 'x-d2' })
    expect(r.task('x').agentId).toBeUndefined()
    const spawn = r.feed({ type: 'salvaged', slug: 'x', agentId: 'x-d2', text: 'The pipeline was restarted.', patch: '/repo/.claude/worktrees/salvage/x-x-d2.patch' })
    const spawned = spawn.actions.find(a => a.type === 'spawn')
    expect(spawned?.type === 'spawn' && spawned.tier).toBe('developer-2')
    expect(spawned?.type === 'spawn' && spawned.prompt).toContain("The previous developer's unsaved changes are in /repo/.claude/worktrees/salvage/x-x-d2.patch: apply what is worth keeping.")
  })

  test('no patch, no mention; a note about a worktree that stays is passed on', async () => {
    const r = new Run()
    r.begin('x')
    r.restart([])
    const plain = r.feed({ type: 'salvaged', slug: 'x', agentId: 'x-d1', text: 'Restarted.' })
    const prompt = plain.actions.find(a => a.type === 'spawn')
    expect(prompt?.type === 'spawn' && prompt.prompt).not.toContain('unsaved')
    const q = new Run()
    q.begin('x')
    q.restart([])
    const kept = q.feed({ type: 'salvaged', slug: 'x', agentId: 'x-d1', text: 'Restarted.', note: 'git diff failed' })
    const withNote = kept.actions.find(a => a.type === 'spawn')
    expect(withNote?.type === 'spawn' && withNote.prompt).toContain('git diff failed')
  })

  test('a listed developer is woken as before; a dead reviewer gets a new one', async () => {
    const r = new Run()
    r.begin('x')
    r.begin('y')
    r.ready('y', 8, HEAD)
    r.poll('y', HEAD, 'success')
    r.feed({ type: 'spawned', slug: 'y', agentId: 'y-rev', role: 'reviewer', tier: 'reviewer' })
    const res = r.restart(['x-d1'])
    expect(res.actions.find(a => a.type === 'tell' && a.agentId === 'x-d1')).toMatchObject({ how: 'send' })
    expect(res.actions.some(a => a.type === 'salvage' && a.slug === 'x')).toBe(false)
    expect(res.actions.find(a => a.type === 'spawn')).toMatchObject({ slug: 'y', role: 'reviewer' })
  })

  test('a developer that was not running is tried with send even when not listed', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'error', text: '' })
    const res = r.restart([])
    expect(res.actions.some(a => a.type === 'salvage')).toBe(false)
    expect(res.actions.find(a => a.type === 'tell')).toMatchObject({ agentId: 'x-d1', how: 'send' })
  })

  test('the driver: the patch path reaches the replacement, an unreferenced worktree is listed to the liaison', async () => {
    const f = fakes()
    const p = createPipeline(f.ports)
    await p.start()
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-2', brief: 'B', after: [] })
    await p.settle()
    const old = f.spawned[0]?.agentId ?? ''
    f.gone.add(old)
    f.server.salvaged.set(old, { patch: `/repo/.claude/worktrees/salvage/feat-${old}.patch` })
    f.server.orphans = ['/repo/.claude/worktrees/agent-zzz']
    const q = createPipeline(f.ports)
    await q.start()
    await q.dispatch({ type: 'resume' })
    await q.settle()
    expect(f.server.calls).toContain(`salvage feat ${old}`)
    expect(f.spawned).toHaveLength(2)
    expect(f.spawned[1]?.prompt).toContain(`feat-${old}.patch`)
    expect(f.notes.some(n => n.includes('/repo/.claude/worktrees/agent-zzz') && n.includes('not removed'))).toBe(true)
  })
})

describe('a bounded task list', () => {
  const cfg = TEST_CONFIG
  const NOW = 100 * DAY

  function stateOf(): State {
    return {
      tasks: [
        oldTask('m1', 'merged', 1 * DAY, { mergedAt: 1 * DAY }),
        oldTask('m2', 'merged', 2 * DAY, { mergedAt: 2 * DAY }),
        oldTask('m3', 'merged', 3 * DAY, { mergedAt: 3 * DAY }),
        oldTask('m4', 'merged', 4 * DAY),
        oldTask('m5', 'merged', 5 * DAY, { mergedAt: 5 * DAY }),
        oldTask('live', 'developing', NOW),
        oldTask('old-blocked', 'blocked', NOW - 15 * DAY, { blockedReason: 'r' }),
        oldTask('new-blocked', 'blocked', NOW - 1 * DAY, { blockedReason: 'r' }),
      ],
    }
  }

  test('without all: tasks that are not merged, the last three merged, the count of the others', async () => {
    const text = describeAll(stateOf(), cfg, NOW)
    const slugs = text.split('\n').map(l => l.split(':')[0])
    expect(slugs).toEqual(['live', 'old-blocked', 'new-blocked', 'm3', 'm4', 'm5', '2 older merged tasks'])
    expect(text).toContain('2 older merged tasks: pipeline_status { all: true }')
  })

  test('with all: every task, no count line', async () => {
    const text = describeAll(stateOf(), cfg, NOW, true)
    expect(text.split('\n')).toHaveLength(8)
    expect(text).not.toContain('older merged')
  })

  test('a blocked task untouched for 14 days is stale', async () => {
    const lines = describeAll(stateOf(), cfg, NOW).split('\n')
    expect(lines.find(l => l.startsWith('old-blocked'))).toContain('old-blocked: blocked stale |')
    expect(lines.find(l => l.startsWith('new-blocked'))).toContain('new-blocked: blocked |')
  })

  test('no tasks, and only a few merged ones: no count line', async () => {
    expect(describeAll(emptyState(), cfg, NOW)).toBe('No tasks.')
    expect(describeAll({ tasks: [oldTask('m1', 'merged', 1)] }, cfg, NOW)).not.toContain('older')
  })

  test('a merged task goes 14 days after its merge; its slug still counts in after', async () => {
    const r = runOf({ tasks: [oldTask('done', 'merged', 0, { mergedAt: 0 }), oldTask('fresh', 'merged', 0, { mergedAt: 10 * DAY })] })
    r.now = 14 * DAY + 5_000
    r.feed({ type: 'tick' })
    expect(r.state.tasks.map(t => t.slug)).toEqual(['fresh'])
    expect(r.state.mergedSlugs).toEqual(['done'])
    // A task that waits for the removed one starts at once; a task that names it is not "unknown".
    const res = r.feed({ type: 'start', slug: 'next', developer: 'developer-1', brief: 'B', after: ['done'] })
    expect(res.error).toBeUndefined()
    expect(r.task('next').stage).toBe('developing')
    expect(r.feed({ type: 'start', slug: 'done', developer: 'developer-1', brief: 'B', after: [] }).error).toContain('exists')
  })

  test('a merged task without mergedAt is aged by lastAt; an old store without the new fields works', async () => {
    const r = runOf({ tasks: [oldTask('old', 'merged', 0), oldTask('waits', 'waiting', 0, { after: ['old'] })] })
    r.now = KEEP_MERGED_MS + 5_000
    r.feed({ type: 'tick' })
    expect(r.state.tasks.map(t => t.slug)).toEqual(['waits'])
    expect(r.task('waits').stage).toBe('developing')
    expect(r.state.mergedSlugs).toEqual(['old'])
  })

  test('only the last 200 slugs are kept', async () => {
    const many = Array.from({ length: 205 }, (_, i) => `s${i}`)
    const r = runOf({ tasks: [oldTask('last', 'merged', 0)], mergedSlugs: many })
    r.now = 15 * DAY
    r.feed({ type: 'tick' })
    expect(r.state.mergedSlugs).toHaveLength(200)
    expect(r.state.mergedSlugs?.[199]).toBe('last')
    expect(r.state.mergedSlugs?.[0]).toBe('s6')
  })

  test('the merge sets mergedAt', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'merged', head: HEAD, ci: 'none', mergeable: true, detail: '', finished: true })
    expect(r.task('x').mergedAt).toBe(r.now)
  })

  test('a blocked task with a pull request is looked at once an hour; one found closed is removed with a notice', async () => {
    const r = runOf({ tasks: [oldTask('b', 'blocked', 0, { pr: 9, blockedReason: 'r' }), oldTask('c', 'blocked', 0, { pr: 10, cleanupPending: true })] })
    r.now = 1_000
    const first = r.feed({ type: 'tick' })
    expect(first.actions.filter(a => a.type === 'poll-closed')).toEqual([{ type: 'poll-closed', slug: 'b', pr: 9 }])
    r.now += CLOSED_POLL_MS / 2
    expect(r.feed({ type: 'tick' }).actions.some(a => a.type === 'poll-closed')).toBe(false)
    r.now += CLOSED_POLL_MS
    expect(r.feed({ type: 'tick' }).actions.some(a => a.type === 'poll-closed')).toBe(true)
    const open = r.feed({ type: 'closed-polled', slug: 'b', pr: 9, prState: 'open' })
    expect(open.actions).toEqual([])
    expect(r.state.tasks.map(t => t.slug)).toEqual(['b', 'c'])
    const closed = r.feed({ type: 'closed-polled', slug: 'b', pr: 9, prState: 'closed' })
    expect(r.state.tasks.map(t => t.slug)).toEqual(['c'])
    expect(closed.actions).toHaveLength(1)
    expect(closed.actions[0]).toMatchObject({ type: 'notify' })
    expect(closed.actions[0]?.type === 'notify' && closed.actions[0].text).toContain('#9 was closed unmerged')
  })

  test('step never changes the state it is given', async () => {
    const before = stateOf()
    const copy = JSON.stringify(before)
    step(before, { type: 'tick', now: NOW }, cfg)
    expect(JSON.stringify(before)).toBe(copy)
  })
})

describe('a send that comes while a turn is ending', () => {
  test('is appended and kept; the end of the turn sends it again in a new turn and the task stays developing', async () => {
    const r = new Run()
    r.begin('x')
    const sent = r.feed({ type: 'send', slug: 'x', text: 'Use the second way.' })
    expect(sent.actions[0]).toMatchObject({ type: 'tell', how: 'append', text: 'Use the second way.' })
    expect(r.task('x').pendingSend).toBe('Use the second way.')
    r.feed({ type: 'send', slug: 'x', text: 'And rename it.' })
    expect(r.task('x').pendingSend).toBe('Use the second way.\n\nAnd rename it.')

    const end = r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Done.\nPIPELINE: pr=7' })
    expect(r.task('x').stage).toBe('developing')
    expect(r.task('x').pendingSend).toBeUndefined()
    expect(r.task('x').pr).toBeUndefined()
    expect(end.actions.some(a => a.type === 'notify' || a.type === 'poll')).toBe(false)
    expect(end.actions).toHaveLength(1)
    const tell = end.actions[0]
    expect(tell).toMatchObject({ type: 'tell', agentId: 'x-d1', how: 'send' })
    expect(tell?.type === 'tell' && tell.text).toContain('A message from the liaison that came while your last turn was ending (ignore it if you already acted on it):')
    expect(tell?.type === 'tell' && tell.text).toContain('Use the second way.\n\nAnd rename it.')

    // The next ending counts.
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Done.\nPIPELINE: pr=7' })
    expect(r.task('x').stage).toBe('checking')
  })

  test('a delivery in a new turn clears it', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'send', slug: 'x', text: 'Hint.' })
    expect(r.task('x').pendingSend).toBeDefined()
    // The developer is resumed by something else (a retry after an API error).
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'error', text: '' })
    r.now += 20 * 60 * 1000
    const tick = r.feed({ type: 'tick' })
    expect(tick.actions.find(a => a.type === 'tell')).toMatchObject({ how: 'send' })
    expect(r.task('x').pendingSend).toBeUndefined()
  })

  test('a send to a developer that is not running is a new turn and keeps nothing', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'error', text: '' })
    r.feed({ type: 'send', slug: 'x', text: 'Hint.' })
    expect(r.task('x').pendingSend).toBeUndefined()
  })

  test('a developer that is gone gets the lost message with its replacement', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'send', slug: 'x', text: 'Hint.' })
    const res = r.feed({ type: 'undelivered', slug: 'x', agentId: 'x-d1', text: 'Something else.' })
    const salvage = res.actions.find(a => a.type === 'salvage')
    expect(salvage?.type === 'salvage' && salvage.text).toContain('Hint.')
    expect(r.task('x').pendingSend).toBeUndefined()
  })
})
