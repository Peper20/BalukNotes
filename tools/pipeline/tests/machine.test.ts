import { describe, expect, test } from 'claude-code/testing'

import { RETRY_MS } from '../hooks/model'
import { parseEnding } from '../hooks/prompts'
import { Run } from './helpers'

const HEAD = 'a'.repeat(40)
const HEAD2 = 'b'.repeat(40)
const HEAD3 = 'c'.repeat(40)

/** A task whose developer ended with a pull request and whose check passed: a reviewer runs. */
function reviewing(r: Run, slug = 'x', pr = 7, head = HEAD, developer: 'developer-1' | 'developer-2' | 'developer-3' = 'developer-1'): void {
  r.begin(slug, developer, `${slug}-dev`)
  r.ready(slug, pr, head)
  r.poll(slug, head, 'success')
  r.feed({ type: 'spawned', slug, agentId: `${slug}-rev`, role: 'reviewer', tier: 'reviewer' })
}

describe('the ending line', () => {
  test('the three endings and a missing one', async () => {
    expect(parseEnding('Done.\nPIPELINE: pr=12')).toEqual({ kind: 'pr', pr: 12, liaison: false })
    expect(parseEnding('Report\n\nPIPELINE: pr=12 liaison\n')).toEqual({ kind: 'pr', pr: 12, liaison: true })
    expect(parseEnding('I stopped.\nPIPELINE: question  ')).toEqual({ kind: 'question' })
    expect(parseEnding('All done, PR 12.')).toBeUndefined()
    // Not exactly the line: quoted in prose, or with extra words.
    expect(parseEnding('End with PIPELINE: pr=<number>\nok')).toBeUndefined()
    expect(parseEnding('PIPELINE: pr=3 please')).toBeUndefined()
    // The last line wins.
    expect(parseEnding('PIPELINE: pr=1\nmore\nPIPELINE: pr=2')).toEqual({ kind: 'pr', pr: 2, liaison: false })
  })
})

describe('developing', () => {
  test('pr=N: the pull request is recorded and checking starts with a poll', async () => {
    const r = new Run()
    r.begin('x')
    expect(r.task('x').stage).toBe('developing')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'ok\nPIPELINE: pr=7' })
    expect(r.task('x').stage).toBe('checking')
    expect(r.task('x').pr).toBe(7)
    expect(r.actions('poll')).toEqual([{ type: 'poll', slug: 'x', pr: 7 }])
    r.poll('x', HEAD, 'pending')
    expect(r.task('x').head).toBe(HEAD)
    expect(r.task('x').stage).toBe('checking')
  })

  test('pr=N liaison: the liaison is told, the task waits', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Additions.\nPIPELINE: pr=7 liaison' })
    expect(r.task('x').stage).toBe('liaison')
    expect(r.task('x').pr).toBe(7)
    expect(r.actions('notify')).toHaveLength(1)
    expect(r.actions('poll')).toHaveLength(0)
  })

  test('question: no pull request, the liaison is told', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Which way?\nPIPELINE: question' })
    expect(r.task('x').stage).toBe('liaison')
    expect(r.task('x').pr).toBeUndefined()
    expect(r.actions('notify')[0]?.text).toContain('x')
  })

  test('no line: asked once to finish, then the liaison', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'I think I am done.' })
    expect(r.task('x').stage).toBe('developing')
    expect(r.actions('tell')).toHaveLength(1)
    expect(r.actions('tell')[0]?.how).toBe('send')
    expect(r.actions('tell')[0]?.text).toContain('PIPELINE')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Still no line.' })
    expect(r.task('x').stage).toBe('liaison')
    expect(r.actions('notify')).toHaveLength(1)
    // No round was counted.
    expect(r.task('x').rounds['developer-1']).toBe(0)
  })

  test('aborted: asked once to finish', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'aborted', text: '' })
    expect(r.actions('tell')).toHaveLength(1)
    expect(r.task('x').nudged).toBe(true)
  })

  test('an API error is retried after a delay and counts nothing', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'error', text: 'Usage limit reached' })
    const at = r.task('x').retryAt ?? 0
    expect(at).toBe(r.now + RETRY_MS)
    expect(r.task('x').nudged).toBe(false)
    expect(r.task('x').rounds['developer-1']).toBe(0)
    expect(r.actions('tell')).toHaveLength(0)
    expect(r.actions('notify')).toHaveLength(1)
    // Too early: nothing.
    r.now = at - 5_000
    r.feed({ type: 'tick' })
    expect(r.actions('tell')).toHaveLength(0)
    // Time is up: the agent is resumed.
    r.now = at
    r.feed({ type: 'tick' })
    expect(r.actions('tell')).toHaveLength(1)
    expect(r.actions('tell')[0]?.agentId).toBe('x-d1')
    expect(r.actions('tell')[0]?.how).toBe('send')
    expect(r.task('x').retryAt).toBeUndefined()
    expect(r.task('x').rounds['developer-1']).toBe(0)
    // A second error does not tell the liaison again.
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'error', text: '' })
    expect(r.actions('notify')).toHaveLength(0)
    expect(r.task('x').retryAt).toBeDefined()
  })
})

describe('checking', () => {
  test('success: a reviewer is spawned for the pull request', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    const res = r.poll('x', HEAD, 'success')
    expect(r.task('x').stage).toBe('reviewing')
    const spawn = res.actions.find(a => a.type === 'spawn')
    expect(spawn).toBeDefined()
    if (spawn?.type === 'spawn') {
      expect(spawn.tier).toBe('reviewer')
      expect(spawn.prompt).toContain('#7')
      expect(spawn.prompt).toContain('Brief of x.')
    }
  })

  test('failure: a round, the developer gets the failed steps', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'failure', 'Full check failed at aaa: clippy.')
    expect(r.task('x').stage).toBe('developing')
    expect(r.task('x').rounds['developer-1']).toBe(1)
    const tell = r.actions('tell')[0]
    expect(tell?.agentId).toBe('x-d1')
    expect(tell?.text).toContain('clippy')
    expect(tell?.text).toContain('PIPELINE')
    // The developer is not running: it is resumed with send.
    expect(tell?.how).toBe('send')
  })

  test('error counts as a failure too; pending waits', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'pending')
    expect(r.task('x').rounds['developer-1']).toBe(0)
    r.poll('x', HEAD, 'error', 'the runner broke')
    expect(r.task('x').rounds['developer-1']).toBe(1)
  })

  test('a new head pushed meanwhile is followed', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'pending')
    r.poll('x', HEAD2, 'pending')
    expect(r.task('x').head).toBe(HEAD2)
    expect(r.task('x').stage).toBe('checking')
    const res = r.poll('x', HEAD2, 'success')
    const spawn = res.actions.find(a => a.type === 'spawn')
    expect(spawn?.type === 'spawn' && spawn.prompt.includes(HEAD2)).toBe(true)
    expect(r.task('x').head).toBe(HEAD2)
  })

  test('a new head while the reviewer works starts the check over', async () => {
    const r = new Run()
    reviewing(r)
    r.feed({ type: 'turn-end', agentId: 'x-rev', reason: 'answer', text: 'Approved.' })
    expect(r.actions('read-review')).toHaveLength(1)
    // The review was read at another head than the one the reviewer got.
    const res = r.review('x', HEAD2, 'APPROVED')
    expect(r.task('x').stage).toBe('checking')
    expect(r.task('x').head).toBe(HEAD2)
    expect(res.actions.some(a => a.type === 'merge')).toBe(false)
    expect(res.actions.some(a => a.type === 'poll')).toBe(true)
  })

  test('one poll that sees the pull request not mergeable is not yet a conflict', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'open', head: HEAD, ci: 'success', mergeable: false, detail: '' })
    // The first time may be the server still checking.
    expect(r.task('x').stage).toBe('reviewing')
  })

  test('conflict detected by two polls', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'open', head: HEAD, ci: 'pending', mergeable: false, detail: '' })
    expect(r.task('x').stage).toBe('checking')
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'open', head: HEAD, ci: 'pending', mergeable: false, detail: '' })
    expect(r.task('x').stage).toBe('developing')
    expect(r.actions('tell')[0]?.text).toContain('Rebase')
    expect(r.task('x').rounds['developer-1']).toBe(0)
    // The developer rebases, pushes a new head and ends again.
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Rebased.\nPIPELINE: pr=7' })
    expect(r.task('x').stage).toBe('checking')
  })
})

describe('reviewing and merging', () => {
  test('approved at the head with a passed status: merge', async () => {
    const r = new Run()
    reviewing(r)
    r.feed({ type: 'turn-end', agentId: 'x-rev', reason: 'answer', text: 'Approved.' })
    const res = r.review('x', HEAD, 'APPROVED')
    expect(r.task('x').stage).toBe('merging')
    expect(res.actions.find(a => a.type === 'merge')).toEqual({ type: 'merge', slug: 'x', pr: 7, head: HEAD })
    // The reviewer's worktree goes at once.
    expect(res.actions.find(a => a.type === 'cleanup')).toEqual({ type: 'cleanup', slug: 'x', mode: 'reviewer', agents: ['x-rev'] })
  })

  test('approved but the status is not success at that head: no merge', async () => {
    const r = new Run()
    reviewing(r)
    const res = r.review('x', HEAD, 'APPROVED', '', 'pending')
    expect(res.actions.some(a => a.type === 'merge')).toBe(false)
    expect(r.task('x').stage).toBe('checking')
  })

  test('an approval of another commit, or a status of another commit, is no approval', async () => {
    for (const mismatch of ['review', 'status'] as const) {
      const r = new Run()
      reviewing(r)
      const t = r.task('x')
      const res = r.feed({
        type: 'reviewed',
        slug: 'x',
        pr: 7,
        head: HEAD,
        verdict: 'APPROVED',
        reviewHead: mismatch === 'review' ? HEAD2 : HEAD,
        text: '',
        status: 'success',
        statusHead: mismatch === 'status' ? HEAD2 : HEAD,
      })
      expect(res.actions.some(a => a.type === 'merge')).toBe(false)
      expect(r.task('x').stage).not.toBe('merging')
      expect(t.stage).toBe('reviewing')
    }
  })

  test('request changes: a round, the review text goes to the developer', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'REQUEST_CHANGES', 'Blocking: the test is missing.')
    expect(r.task('x').stage).toBe('developing')
    expect(r.task('x').rounds['developer-1']).toBe(1)
    expect(r.actions('tell')[0]?.text).toContain('the test is missing')
    expect(r.actions('tell')[0]?.agentId).toBe('x-dev')
  })

  test('no verdict: the liaison is told', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'none')
    expect(r.task('x').stage).toBe('liaison')
    expect(r.actions('notify')).toHaveLength(1)
    expect(r.actions('merge')).toHaveLength(0)
  })

  test('the reviewer turn ends: its review is read; an API error is retried', async () => {
    const r = new Run()
    reviewing(r)
    r.feed({ type: 'turn-end', agentId: 'x-rev', reason: 'error', text: '' })
    expect(r.actions('read-review')).toHaveLength(0)
    expect(r.task('x').retryAt).toBeDefined()
    r.now = r.task('x').retryAt ?? 0
    r.feed({ type: 'tick' })
    expect(r.actions('tell')[0]?.agentId).toBe('x-rev')
  })

  test('an approval at a head whose status is pending: the status passing later merges it, no second reviewer', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'APPROVED', '', 'pending')
    expect(r.task('x').stage).toBe('checking')
    expect(r.task('x').approvedHead).toBe(HEAD)
    const res = r.poll('x', HEAD, 'success')
    expect(r.task('x').stage).toBe('merging')
    expect(res.actions.find(a => a.type === 'merge')).toEqual({ type: 'merge', slug: 'x', pr: 7, head: HEAD })
    expect(res.actions.some(a => a.type === 'spawn')).toBe(false)
  })

  test('a new head after an approval clears it: the new head is reviewed', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'APPROVED', '', 'pending')
    const res = r.poll('x', HEAD2, 'success')
    expect(r.task('x').approvedHead).toBeUndefined()
    expect(r.task('x').stage).toBe('reviewing')
    expect(res.actions.some(a => a.type === 'merge')).toBe(false)
    expect(res.actions.find(a => a.type === 'spawn')).toMatchObject({ role: 'reviewer' })
  })

  test('an approved head that fails the check goes back to the developer', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'APPROVED', '', 'pending')
    r.poll('x', HEAD, 'failure', 'clippy')
    expect(r.task('x').stage).toBe('developing')
    expect(r.task('x').rounds['developer-1']).toBe(1)
    expect(r.actions('merge')).toHaveLength(0)
  })

  test('a head pushed during the review drops the approval record', async () => {
    const r = new Run()
    reviewing(r)
    r.task('x').approvedHead = HEAD
    r.feed({ type: 'reviewed', slug: 'x', pr: 7, head: HEAD2, verdict: 'APPROVED', reviewHead: HEAD, text: '', status: 'success', statusHead: HEAD2 })
    expect(r.task('x').approvedHead).toBeUndefined()
    expect(r.task('x').stage).toBe('checking')
  })

  test('the rebase message allows the forced push only to the task branch', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'open', head: HEAD, ci: 'pending', mergeable: false, detail: '' })
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'open', head: HEAD, ci: 'pending', mergeable: false, detail: '' })
    const text = r.actions('tell')[0]?.text ?? ''
    expect(text).toContain('--force-with-lease')
    expect(text).toContain("only to this task's own branch task/x, never to main or to another branch")
    expect(text).toContain('merge origin/main into the branch')
  })

  test('a merge that fails with a conflict sends the developer to rebase', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'APPROVED')
    r.feed({ type: 'merge-result', slug: 'x', ok: false, kind: 'conflict', message: 'conflict' })
    expect(r.task('x').stage).toBe('developing')
    expect(r.task('x').rounds['developer-1']).toBe(0)
    expect(r.actions('tell')[0]?.text).toContain('Rebase')
  })

  test('a merge that fails otherwise goes to the liaison', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'APPROVED')
    r.feed({ type: 'merge-result', slug: 'x', ok: false, kind: 'other', message: 'server said no' })
    expect(r.task('x').stage).toBe('liaison')
    expect(r.actions('notify')[0]?.text).toContain('server said no')
  })
})

describe('rounds', () => {
  test('two rounds of a developer, then developer-3 takes over on the same branch and pull request', async () => {
    const r = new Run()
    r.begin('x', 'developer-1', 'x-d1')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'failure', 'clippy')
    expect(r.task('x').rounds['developer-1']).toBe(1)
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Fixed.\nPIPELINE: pr=7' })
    r.poll('x', HEAD2, 'failure', 'fmt')
    expect(r.task('x').rounds['developer-1']).toBe(2)
    expect(r.task('x').developer).toBe('developer-1')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Fixed.\nPIPELINE: pr=7' })
    const res = r.poll('x', HEAD3, 'failure', 'tests broke')
    // The third return goes to a new developer-3.
    expect(r.task('x').developer).toBe('developer-3')
    expect(r.task('x').rounds['developer-1']).toBe(2)
    expect(r.task('x').rounds['developer-3']).toBe(1)
    expect(r.task('x').agentId).toBeUndefined()
    const handover = res.actions.find(a => a.type === 'handover')
    expect(handover).toBeDefined()
    if (handover?.type === 'handover') {
      expect(handover.previous).toBe('x-d1')
      expect(handover.prompt).toContain('task/x')
      expect(handover.prompt).toContain('#7')
      expect(handover.prompt).toContain('tests broke')
      expect(handover.prompt).toContain('PIPELINE')
    }
    expect(res.actions.some(a => a.type === 'tell')).toBe(false)
    r.feed({ type: 'spawned', slug: 'x', agentId: 'x-d3', role: 'developer', tier: 'developer-3' })
    expect(r.task('x').agentId).toBe('x-d3')
    expect(r.task('x').stage).toBe('developing')
  })

  test('review requests count as rounds too', async () => {
    const r = new Run()
    reviewing(r)
    r.review('x', HEAD, 'REQUEST_CHANGES', 'one')
    expect(r.task('x').rounds['developer-1']).toBe(1)
    r.feed({ type: 'turn-end', agentId: 'x-dev', reason: 'answer', text: 'ok\nPIPELINE: pr=7' })
    r.poll('x', HEAD2, 'failure', 'check')
    expect(r.task('x').rounds['developer-1']).toBe(2)
  })

  test('after the two rounds of developer-3 the task is blocked', async () => {
    const r = new Run()
    r.begin('x', 'developer-3', 'x-d3')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'failure', 'one')
    expect(r.task('x').rounds['developer-3']).toBe(1)
    r.feed({ type: 'turn-end', agentId: 'x-d3', reason: 'answer', text: 'ok\nPIPELINE: pr=7' })
    r.poll('x', HEAD2, 'failure', 'two')
    expect(r.task('x').rounds['developer-3']).toBe(2)
    r.feed({ type: 'turn-end', agentId: 'x-d3', reason: 'answer', text: 'ok\nPIPELINE: pr=7' })
    const res = r.poll('x', HEAD3, 'failure', 'three')
    expect(r.task('x').stage).toBe('blocked')
    expect(r.task('x').rounds['developer-3']).toBe(2)
    expect(res.actions.some(a => a.type === 'tell')).toBe(false)
    expect(res.actions.find(a => a.type === 'notify')).toBeDefined()
    // The pull request and the branch stay; a comment says why, and the local side is cleaned.
    const comment = res.actions.find(a => a.type === 'comment')
    expect(comment?.type === 'comment' && comment.text.includes('three')).toBe(true)
    const cleanup = res.actions.find(a => a.type === 'cleanup')
    expect(cleanup?.type === 'cleanup' && cleanup.mode).toBe('blocked')
  })

  test('the full ladder: developer-2, handover, developer-3, block', async () => {
    const r = new Run()
    r.begin('x', 'developer-2', 'x-d2')
    r.ready('x', 7, HEAD)
    let head = HEAD
    const heads = ['1', '2', '3', '4', '5'].map(c => c.repeat(40))
    for (const [i, h] of heads.entries()) {
      head = h
      r.poll('x', head, 'failure', `fail ${i}`)
      const t = r.task('x')
      if (t.stage === 'blocked') break
      const id = t.agentId ?? 'x-d3'
      if (t.agentId === undefined) r.feed({ type: 'spawned', slug: 'x', agentId: id, role: 'developer', tier: t.developer })
      r.feed({ type: 'turn-end', agentId: id, reason: 'answer', text: 'ok\nPIPELINE: pr=7' })
    }
    expect(r.task('x').stage).toBe('blocked')
    expect(r.task('x').rounds).toEqual({ 'developer-1': 0, 'developer-2': 2, 'developer-3': 2 })
  })
})

describe('dependencies', () => {
  test('after: the task waits until every listed task is merged', async () => {
    const r = new Run()
    r.begin('a')
    const waiting = r.feed({ type: 'start', slug: 'b', developer: 'developer-2', brief: 'B', after: ['a'] })
    expect(r.task('b').stage).toBe('waiting')
    expect(waiting.actions).toHaveLength(0)
    expect(waiting.reply).toContain('waits')
    // a is merged: b starts with the developer of its tier.
    r.ready('a', 5, HEAD)
    r.poll('a', HEAD, 'success')
    r.feed({ type: 'spawned', slug: 'a', agentId: 'a-rev', role: 'reviewer', tier: 'reviewer' })
    r.review('a', HEAD, 'APPROVED')
    r.feed({ type: 'merge-result', slug: 'a', ok: true, head: HEAD })
    expect(r.task('b').stage).toBe('waiting')
    const res = r.feed({ type: 'post-merge', slug: 'a', pull: 'ok', install: 'skipped', note: '', leftovers: [], conflicted: [] })
    expect(r.task('b').stage).toBe('developing')
    const spawn = res.actions.find(a => a.type === 'spawn')
    expect(spawn?.type === 'spawn' && spawn.tier).toBe('developer-2')
    expect(spawn?.type === 'spawn' && spawn.prompt).toContain('task/b')
    const report = res.actions.find(a => a.type === 'notify')
    expect(report?.type === 'notify' && report.text).toContain('http://localhost:3000/ibaluk/baluk-notes/pulls/5')
    expect(report?.type === 'notify' && report.text).toContain('started: b')
  })

  test('two dependencies: both must be merged', async () => {
    const r = new Run()
    r.begin('a')
    r.begin('b')
    r.feed({ type: 'start', slug: 'c', developer: 'developer-1', brief: 'C', after: ['a', 'b'] })
    for (const [slug, head] of [['a', HEAD], ['b', HEAD2]] as const) {
      r.ready(slug, slug === 'a' ? 5 : 6, head)
      r.poll(slug, head, 'success')
      r.feed({ type: 'spawned', slug, agentId: `${slug}-rev`, role: 'reviewer', tier: 'reviewer' })
      r.review(slug, head, 'APPROVED')
      r.feed({ type: 'merge-result', slug, ok: true, head })
      r.feed({ type: 'post-merge', slug, pull: 'ok', install: 'skipped', note: '', leftovers: [], conflicted: [] })
      expect(r.task('c').stage).toBe(slug === 'a' ? 'waiting' : 'developing')
    }
  })

  test('unknown or duplicate tasks are refused', async () => {
    const r = new Run()
    r.begin('a')
    expect(r.feed({ type: 'start', slug: 'a', developer: 'developer-1', brief: 'x', after: [] }).error).toContain('exists')
    expect(r.feed({ type: 'start', slug: 'b', developer: 'developer-1', brief: 'x', after: ['nope'] }).error).toContain('nope')
    expect(r.feed({ type: 'start', slug: 'Bad Slug', developer: 'developer-1', brief: 'x', after: [] }).error).toContain('bad slug')
  })
})

describe('after a merge', () => {
  test('another pull request that conflicts sends its developer to rebase; then checking again', async () => {
    const r = new Run()
    r.begin('a', 'developer-1', 'a-dev')
    r.begin('b', 'developer-1', 'b-dev')
    r.ready('a', 5, HEAD)
    r.ready('b', 6, HEAD2)
    r.poll('a', HEAD, 'success')
    r.feed({ type: 'spawned', slug: 'a', agentId: 'a-rev', role: 'reviewer', tier: 'reviewer' })
    r.review('a', HEAD, 'APPROVED')
    r.feed({ type: 'merge-result', slug: 'a', ok: true, head: HEAD })
    const res = r.feed({ type: 'post-merge', slug: 'a', pull: 'ok', install: 'ok', note: '', leftovers: [], conflicted: ['b'] })
    expect(r.task('b').stage).toBe('developing')
    expect(r.task('b').rounds['developer-1']).toBe(0)
    const tell = res.actions.find(a => a.type === 'tell')
    expect(tell?.type === 'tell' && tell.agentId).toBe('b-dev')
    expect(tell?.type === 'tell' && tell.text).toContain('Rebase')
    const report = res.actions.find(a => a.type === 'notify')
    expect(report?.type === 'notify' && report.text).toContain('b told to rebase')
    r.feed({ type: 'turn-end', agentId: 'b-dev', reason: 'answer', text: 'Rebased.\nPIPELINE: pr=6' })
    expect(r.task('b').stage).toBe('checking')
  })

  test('the report names rounds, the install result and the cleanup leftovers', async () => {
    const r = new Run()
    r.begin('a')
    r.ready('a', 5, HEAD)
    r.poll('a', HEAD, 'failure', 'x')
    r.feed({ type: 'turn-end', agentId: 'a-d1', reason: 'answer', text: 'ok\nPIPELINE: pr=5' })
    r.poll('a', HEAD2, 'success')
    r.feed({ type: 'spawned', slug: 'a', agentId: 'a-rev', role: 'reviewer', tier: 'reviewer' })
    r.review('a', HEAD2, 'APPROVED')
    r.feed({ type: 'merge-result', slug: 'a', ok: true, head: HEAD2 })
    const res = r.feed({
      type: 'post-merge',
      slug: 'a',
      pull: 'ok',
      install: 'failed',
      note: 'exit 1',
      leftovers: ['worktree agent-a-d1: has changes'],
      conflicted: [],
    })
    const text = res.actions.find(a => a.type === 'notify')
    expect(text?.type === 'notify' && text.text).toContain('developer-1 1')
    expect(text?.type === 'notify' && text.text).toContain('install: failed (exit 1)')
    expect(text?.type === 'notify' && text.text).toContain('worktree agent-a-d1')
    expect(r.task('a').stage).toBe('merged')
  })
})

describe('the liaison', () => {
  test('pipeline_send resumes a task that waits for the liaison', async () => {
    const r = new Run()
    r.begin('x')
    r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Which way?\nPIPELINE: question' })
    const res = r.feed({ type: 'send', slug: 'x', text: 'Take the first.' })
    expect(r.task('x').stage).toBe('developing')
    expect(res.actions[0]).toMatchObject({ type: 'tell', agentId: 'x-d1', text: 'Take the first.', how: 'send' })
  })

  test('a message to a running developer is appended', async () => {
    const r = new Run()
    r.begin('x')
    const res = r.feed({ type: 'send', slug: 'x', text: 'Also this.' })
    expect(res.actions[0]).toMatchObject({ type: 'tell', how: 'append' })
    expect(r.task('x').stage).toBe('developing')
  })

  test('a developer that cannot be reached is replaced by a new one of the same tier', async () => {
    const r = new Run()
    r.begin('x', 'developer-2', 'x-d2')
    r.feed({ type: 'turn-end', agentId: 'x-d2', reason: 'answer', text: 'Q\nPIPELINE: question' })
    r.feed({ type: 'send', slug: 'x', text: 'Answer.' })
    const res = r.feed({ type: 'undelivered', slug: 'x', agentId: 'x-d2', text: 'Answer.' })
    const spawn = res.actions.find(a => a.type === 'spawn')
    expect(spawn?.type === 'spawn' && spawn.tier).toBe('developer-2')
    expect(spawn?.type === 'spawn' && spawn.prompt).toContain('already exists')
    expect(spawn?.type === 'spawn' && spawn.prompt).toContain('Answer.')
  })

  test('stop: running agents are told to commit and push, the task is blocked, nothing new starts', async () => {
    const r = new Run()
    r.begin('x')
    r.begin('y')
    const res = r.feed({ type: 'stop', reason: 'the user said so' })
    expect(r.task('x').stage).toBe('blocked')
    expect(r.task('y').stage).toBe('blocked')
    const tells = res.actions.filter(a => a.type === 'tell')
    expect(tells).toHaveLength(2)
    expect(tells.every(a => a.type === 'tell' && a.how === 'append' && a.text.toLowerCase().includes('commit and push'))).toBe(true)
    expect(res.actions.some(a => a.type === 'spawn')).toBe(false)
    // The local cleanup waits until the agents have stopped.
    expect(res.actions.some(a => a.type === 'cleanup')).toBe(false)
    const end = r.feed({ type: 'turn-end', agentId: 'x-d1', reason: 'answer', text: 'Pushed.' })
    expect(end.actions.find(a => a.type === 'cleanup')).toMatchObject({ slug: 'x', mode: 'blocked' })
    expect(r.task('x').stage).toBe('blocked')
  })

  test('stop of one task leaves the others', async () => {
    const r = new Run()
    r.begin('x')
    r.begin('y')
    r.feed({ type: 'stop', slug: 'x', reason: 'r' })
    expect(r.task('x').stage).toBe('blocked')
    expect(r.task('y').stage).toBe('developing')
    expect(r.feed({ type: 'stop', slug: 'nope', reason: 'r' }).error).toContain('nope')
  })

  test('a blocked task resumed by the liaison continues on a new developer-3 with fresh rounds', async () => {
    const r = new Run()
    r.begin('x', 'developer-3', 'x-d3')
    r.feed({ type: 'stop', reason: 'r' })
    r.feed({ type: 'turn-end', agentId: 'x-d3', reason: 'answer', text: 'Pushed.' })
    const res = r.feed({ type: 'send', slug: 'x', text: 'Go on, the user agreed.' })
    expect(r.task('x').stage).toBe('developing')
    expect(r.task('x').rounds['developer-3']).toBe(0)
    const spawn = res.actions.find(a => a.type === 'spawn')
    expect(spawn?.type === 'spawn' && spawn.tier).toBe('developer-3')
  })
})

describe('restart', () => {
  test('the store is reloaded: a developer is tried with send, a lost one is replaced', async () => {
    const r = new Run()
    r.begin('x', 'developer-2', 'old-agent')
    r.begin('w')
    r.feed({ type: 'start', slug: 'z', developer: 'developer-1', brief: 'Z', after: ['x'] })
    const res = r.feed({ type: 'restore' })
    const tell = res.actions.find(a => a.type === 'tell' && a.agentId === 'old-agent')
    expect(tell).toMatchObject({ type: 'tell', how: 'send' })
    const lost = r.feed({ type: 'undelivered', slug: 'x', agentId: 'old-agent', text: 'restarted' })
    const spawn = lost.actions.find(a => a.type === 'spawn')
    expect(spawn?.type === 'spawn' && spawn.tier).toBe('developer-2')
    expect(r.task('z').stage).toBe('waiting')
  })

  test('a task in checking is polled again', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    const res = r.feed({ type: 'restore' })
    expect(res.actions.some(a => a.type === 'poll')).toBe(true)
  })
})
