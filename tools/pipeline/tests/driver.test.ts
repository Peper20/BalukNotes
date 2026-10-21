import { describe, expect, test } from 'claude-code/testing'

import { createPipeline } from '../hooks/driver'
import { fakes, type Fakes } from './helpers'

const HEAD = 'a'.repeat(40)
const HEAD2 = 'b'.repeat(40)

async function started(f: Fakes) {
  const p = createPipeline(f.ports)
  await p.start()
  return p
}

type P = Awaited<ReturnType<typeof started>>

/** A task up to the reviewer's turn end: the developer got a pull request that passed the check. */
async function toReview(f: Fakes, p: P, slug: string, pr: number, head: string, developer: 'developer-1' | 'developer-2' = 'developer-1') {
  await p.dispatch({ type: 'start', slug, developer, brief: `Brief of ${slug}.`, after: [] })
  await p.settle()
  const dev = f.spawned[f.spawned.length - 1]?.agentId ?? ''
  f.server.open(pr, head)
  f.server.setStatus(head, 'success')
  await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: `Done.\nPIPELINE: pr=${pr}` })
  await p.settle()
  const rev = f.spawned[f.spawned.length - 1]?.agentId ?? ''
  return { dev, rev }
}

/** The reviewer ends its turn: its review (if the test posted one) is read. */
async function finish(p: P, rev: string) {
  await p.dispatch({ type: 'turn-end', agentId: rev, reason: 'answer', text: 'Verdict given.' })
  await p.settle()
}

describe('a task from start to merge', () => {
  test('developer, check, reviewer, merge, cleanup, pull, report', async () => {
    const f = fakes()
    const p = await started(f)
    f.server.pull = { pull: 'ok', note: '', changed: ['docs/agents.md'] }
    const { dev, rev } = await toReview(f, p, 'feat', 7, HEAD)
    expect(f.spawned.map(s => s.tier)).toEqual(['developer-1', 'reviewer'])
    expect(f.spawned[0]?.prompt).toContain('Brief of feat.')
    expect(f.spawned[0]?.prompt).toContain('task/feat')
    expect(f.spawned[0]?.prompt).toContain('PIPELINE: pr=<number>')
    expect(f.spawned[1]?.prompt).toContain('#7')
    f.server.review(7, HEAD, 'APPROVED')
    await finish(p, rev)
    expect(p.state().tasks[0]?.stage).toBe('merged')
    expect(f.server.calls).toEqual([
      `removeAgents ${rev}`,
      `merge 7 ${HEAD} task/feat`,
      `removeAgents ${dev},${rev}`,
      'cleanupMerged feat 7',
      'pullMain',
    ])
    // The reviewer's worktree was also removed at once, after its verdict.
    expect(f.notes).toHaveLength(1)
    expect(f.notes[0]).toContain('http://localhost:3000/ibaluk/baluk-notes/pulls/7')
    expect(f.notes[0]).toContain('install: skipped')
    // The tasks are in the store.
    expect(f.store.value?.tasks[0]?.slug).toBe('feat')
  })

  test('install runs when the merged diff touches crates/, app/, baluk/ or skills/', async () => {
    for (const path of ['crates/notes-core/src/lib.rs', 'app/src/a.ts', 'baluk/x.typ', 'skills/baluk-note/SKILL.md']) {
      const f = fakes()
      const p = await started(f)
      f.server.pull = { pull: 'ok', note: '', changed: [path, 'docs/a.md'] }
      const { rev } = await toReview(f, p, 'feat', 7, HEAD)
      f.server.review(7, HEAD, 'APPROVED')
      await finish(p, rev)
      expect(f.server.calls).toContain('install')
      expect(f.notes[0]).toContain('install: ok')
    }
  })

  test('a failed install and a failed pull are reported', async () => {
    const f = fakes()
    const p = await started(f)
    f.server.pull = { pull: 'ok', note: '', changed: ['crates/a.rs'] }
    f.server.installOk = false
    const { rev } = await toReview(f, p, 'feat', 7, HEAD)
    f.server.review(7, HEAD, 'APPROVED')
    await finish(p, rev)
    expect(f.notes[0]).toContain('install: failed (broken)')
    const g = fakes()
    const q = await started(g)
    g.server.pull = { pull: 'failed', note: 'not fast-forward', changed: [] }
    const { rev: rev2 } = await toReview(g, q, 'feat', 7, HEAD)
    g.server.review(7, HEAD, 'APPROVED')
    await finish(q, rev2)
    expect(g.server.calls).not.toContain('install')
    expect(g.notes[0]).toContain('pull failed')
  })

  test('cleanup leftovers go into the report', async () => {
    const f = fakes()
    const p = await started(f)
    f.server.leftovers = ['worktree agent-x: has changes']
    const { rev } = await toReview(f, p, 'feat', 7, HEAD)
    f.server.review(7, HEAD, 'APPROVED')
    await finish(p, rev)
    expect(f.notes[0]).toContain('worktree agent-x: has changes')
  })
})

describe('what the machine is never allowed to do', () => {
  test('no merge without a verdict on the server', async () => {
    const f = fakes()
    const p = await started(f)
    const { rev } = await toReview(f, p, 'feat', 7, HEAD)
    await finish(p, rev)
    expect(f.server.calls.filter(c => c.startsWith('merge'))).toHaveLength(0)
    expect(p.state().tasks[0]?.stage).toBe('liaison')
    expect(f.notes.some(n => n.includes('without a verdict'))).toBe(true)
  })

  test('no merge on an approval of an older commit', async () => {
    const f = fakes()
    const p = await started(f)
    const { rev } = await toReview(f, p, 'feat', 7, HEAD)
    // The approval is for HEAD, but the developer has pushed HEAD2 since.
    f.server.review(7, HEAD, 'APPROVED')
    f.server.push(7, HEAD2)
    f.server.setStatus(HEAD2, 'success')
    await finish(p, rev)
    expect(f.server.calls.filter(c => c.startsWith('merge'))).toHaveLength(0)
    // The old review is stale: the new head is checked and gets a reviewer of its own.
    expect(p.state().tasks[0]?.head).toBe(HEAD2)
    expect(p.state().tasks[0]?.stage).toBe('reviewing')
    expect(f.spawned.map(x => x.tier)).toEqual(['developer-1', 'reviewer', 'reviewer'])
    expect(f.spawned[2]?.prompt).toContain(HEAD2)
  })

  test('no merge when the status is not success at the head', async () => {
    const f = fakes()
    const p = await started(f)
    const { rev } = await toReview(f, p, 'feat', 7, HEAD)
    f.server.setStatus(HEAD, 'pending')
    f.server.review(7, HEAD, 'APPROVED')
    await finish(p, rev)
    expect(f.server.calls.filter(c => c.startsWith('merge'))).toHaveLength(0)
    expect(p.state().tasks[0]?.stage).toBe('checking')
  })

  test('requested changes go to the developer with the review text; no merge', async () => {
    const f = fakes()
    const p = await started(f)
    const { dev, rev } = await toReview(f, p, 'feat', 7, HEAD)
    f.server.review(7, HEAD, 'REQUEST_CHANGES', 'Blocking: no test.')
    await finish(p, rev)
    expect(f.sent.find(s => s.agentId === dev)?.text).toContain('Blocking: no test.')
    expect(p.state().tasks[0]?.stage).toBe('developing')
    expect(f.server.calls.filter(c => c.startsWith('merge'))).toHaveLength(0)
  })
})

describe('checking', () => {
  test('the status is polled each tick until it is final', async () => {
    const f = fakes()
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    f.server.open(7, HEAD)
    const dev = f.spawned[0]?.agentId ?? ''
    await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'PIPELINE: pr=7' })
    await p.settle()
    expect(p.state().tasks[0]?.stage).toBe('checking')
    await p.tick()
    await p.settle()
    expect(p.state().tasks[0]?.stage).toBe('checking')
    f.server.setStatus(HEAD, 'success')
    await p.tick()
    await p.settle()
    expect(p.state().tasks[0]?.stage).toBe('reviewing')
    expect(f.spawned.map(s => s.tier)).toEqual(['developer-1', 'reviewer'])
  })

  test('a failed check goes to the developer with the failure; handover after two rounds', async () => {
    const f = fakes()
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    const dev = f.spawned[0]?.agentId ?? ''
    f.server.open(7, HEAD)
    f.server.setStatus(HEAD, 'failure', 'Full check failed at aaa: clippy.')
    await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'PIPELINE: pr=7' })
    await p.settle()
    expect(f.sent.find(s => s.agentId === dev)?.text).toContain('clippy')
    // Round 2.
    f.server.push(7, HEAD2)
    f.server.setStatus(HEAD2, 'failure', 'fmt')
    await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'PIPELINE: pr=7' })
    await p.settle()
    expect(p.state().tasks[0]?.rounds['developer-1']).toBe(2)
    // The third failure: the previous developer's worktree is removed, developer-3 starts.
    const head3 = 'c'.repeat(40)
    f.server.push(7, head3)
    f.server.setStatus(head3, 'failure', 'tests')
    await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'PIPELINE: pr=7' })
    await p.settle()
    expect(f.server.calls).toContain(`removeAgents ${dev}`)
    const last = f.spawned[f.spawned.length - 1]
    expect(last?.tier).toBe('developer-3')
    expect(last?.prompt).toContain('tests')
    expect(last?.prompt).toContain('#7')
    expect(p.state().tasks[0]?.developer).toBe('developer-3')
    expect(p.state().tasks[0]?.agentId).toBe(last?.agentId)
  })

  test('a hand-over whose worktree cannot be removed blocks the task', async () => {
    const f = fakes()
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    const dev = f.spawned[0]?.agentId ?? ''
    f.server.open(7, HEAD)
    let head = HEAD
    f.server.setStatus(head, 'failure', 'one')
    for (const next of [HEAD2, 'c'.repeat(40)]) {
      await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'PIPELINE: pr=7' })
      await p.settle()
      head = next
      f.server.push(7, head)
      f.server.setStatus(head, 'failure', 'again')
    }
    f.server.leftovers = ['worktree agent-x: contains modified files']
    await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'PIPELINE: pr=7' })
    await p.settle()
    expect(p.state().tasks[0]?.stage).toBe('blocked')
    expect(f.notes.some(n => n.includes('hand-over'))).toBe(true)
  })

  test('an agent that cannot be reached is replaced by a new one of the same tier', async () => {
    const f = fakes()
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-2', brief: 'B', after: [] })
    await p.settle()
    const dev = f.spawned[0]?.agentId ?? ''
    f.server.open(7, HEAD)
    f.server.setStatus(HEAD, 'failure', 'clippy')
    f.gone.add(dev)
    await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'PIPELINE: pr=7' })
    await p.settle()
    const last = f.spawned[f.spawned.length - 1]
    expect(f.spawned).toHaveLength(2)
    expect(last?.tier).toBe('developer-2')
    expect(last?.prompt).toContain('clippy')
    expect(last?.prompt).toContain('already exists')
    expect(p.state().tasks[0]?.agentId).toBe(last?.agentId)
  })
})

describe('several tasks', () => {
  test('after: the dependent task starts when the merge is done', async () => {
    const f = fakes()
    const p = await started(f)
    const { rev } = await toReview(f, p, 'a', 5, HEAD)
    await p.dispatch({ type: 'start', slug: 'c', developer: 'developer-3', brief: 'C', after: ['a'] })
    await p.settle()
    expect(f.spawned.some(s => s.tier === 'developer-3')).toBe(false)
    f.server.review(5, HEAD, 'APPROVED')
    await finish(p, rev)
    expect(f.spawned.filter(s => s.tier === 'developer-3')).toHaveLength(1)
    expect(p.state().tasks.find(t => t.slug === 'c')?.stage).toBe('developing')
    expect(f.notes[0]).toContain('started: c')
  })

  test('a conflict after a merge: the developer of the other pull request rebases', async () => {
    const f = fakes()
    const p = await started(f)
    // Task b is in checking with its pull request.
    await p.dispatch({ type: 'start', slug: 'b', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    const bDev = f.spawned[0]?.agentId ?? ''
    f.server.open(9, HEAD2)
    await p.dispatch({ type: 'turn-end', agentId: bDev, reason: 'answer', text: 'PIPELINE: pr=9' })
    await p.settle()
    expect(p.state().tasks[0]?.stage).toBe('checking')
    // Task a is merged; b now conflicts.
    const { rev } = await toReview(f, p, 'a', 5, HEAD)
    f.server.conflicting.add(9)
    f.server.review(5, HEAD, 'APPROVED')
    await finish(p, rev)
    const b = p.state().tasks.find(t => t.slug === 'b')
    expect(b?.stage).toBe('developing')
    expect(b?.rounds['developer-1']).toBe(0)
    expect(f.sent.find(s => s.agentId === bDev)?.text).toContain('Rebase')
    expect(f.notes[0]).toContain('b told to rebase')
  })
})

describe('the session restarts', () => {
  test('tasks come back from the store; an agent of the old session is tried, a lost one replaced', async () => {
    const f = fakes()
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-2', brief: 'B', after: [] })
    await p.settle()
    const old = f.spawned[0]?.agentId ?? ''
    // A new session: the store is kept, the agent is not reachable.
    f.gone.add(old)
    const q = await started(f)
    await q.settle()
    expect(q.state().tasks).toHaveLength(1)
    expect(f.spawned).toHaveLength(2)
    expect(f.spawned[1]?.tier).toBe('developer-2')
    expect(f.spawned[1]?.prompt).toContain('already exists')
    expect(q.state().tasks[0]?.agentId).toBe(f.spawned[1]?.agentId)
  })

  test('a reachable agent is woken with send', async () => {
    const f = fakes()
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    const old = f.spawned[0]?.agentId ?? ''
    const q = await started(f)
    await q.settle()
    expect(f.sent.find(s => s.agentId === old)).toBeDefined()
    expect(f.spawned).toHaveLength(1)
  })
})

describe('refusals and stop', () => {
  test('a spawn the engine refuses goes to the liaison', async () => {
    const f = fakes()
    f.denySpawn.reason = 'no such agent'
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    expect(p.state().tasks[0]?.stage).toBe('liaison')
    expect(f.notes[0]).toContain('no such agent')
  })

  test('stop tells the running developer, comments on the pull request and cleans up the local side', async () => {
    const f = fakes()
    const p = await started(f)
    await p.dispatch({ type: 'start', slug: 'feat', developer: 'developer-1', brief: 'B', after: [] })
    await p.settle()
    const dev = f.spawned[0]?.agentId ?? ''
    await p.dispatch({ type: 'stop', reason: 'the user said stop' })
    await p.settle()
    expect(f.appended.find(a => a.agentId === dev)?.text).toContain('the user said stop')
    expect(f.server.calls.some(c => c.startsWith('cleanupBlocked'))).toBe(false)
    await p.dispatch({ type: 'turn-end', agentId: dev, reason: 'answer', text: 'Pushed.' })
    await p.settle()
    expect(f.server.calls).toContain(`removeAgents ${dev}`)
    expect(f.server.calls).toContain('cleanupBlocked feat')
    expect(p.state().tasks[0]?.stage).toBe('blocked')
    expect(f.spawned).toHaveLength(1)
  })
})
