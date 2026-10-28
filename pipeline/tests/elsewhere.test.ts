import { describe, expect, test } from 'claude-code/testing'

import { Run } from './helpers'

const HEAD = 'a'.repeat(40)

describe('merges made elsewhere', () => {
  test('a pull request found merged by a poll is cleaned up and reported', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    const res = r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'merged', head: HEAD, ci: 'none', mergeable: true, detail: '', finished: true })
    expect(r.task('x').stage).toBe('merged')
    expect(res.actions).toEqual([{ type: 'post-merge', slug: 'x', pr: 7, head: HEAD }])
  })

  test('a pull request closed on the server blocks the task', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.feed({ type: 'polled', slug: 'x', pr: 7, prState: 'closed', head: HEAD, ci: 'none', mergeable: true, detail: '', finished: true })
    expect(r.task('x').stage).toBe('blocked')
  })

  test('a session that ended between the merge and its cleanup does the cleanup on restore', async () => {
    const r = new Run()
    r.begin('x')
    r.ready('x', 7, HEAD)
    r.poll('x', HEAD, 'success')
    r.feed({ type: 'spawned', slug: 'x', agentId: 'x-rev', role: 'reviewer', tier: 'reviewer' })
    r.review('x', HEAD, 'APPROVED')
    r.feed({ type: 'merge-result', slug: 'x', ok: true, head: HEAD })
    const res = r.feed({ type: 'restore', agents: [] })
    expect(res.actions).toEqual([{ type: 'post-merge', slug: 'x', pr: 7, head: HEAD }])
  })
})
