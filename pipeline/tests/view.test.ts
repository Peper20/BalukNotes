// view.ts: the status line from the counts of the tasks by stage.
import { describe, expect, test } from 'claude-code/testing'

import type { Stage, State, Task } from '../hooks/model'
import { describeTask, summary } from '../hooks/view'
import { TEST_CONFIG } from './helpers'

/** A task in `stage`; the other fields are those of a fresh task. */
function task(slug: string, stage: Stage): Task {
  return {
    slug,
    brief: '',
    after: [],
    stage,
    starter: 'developer-1',
    developer: 'developer-1',
    agents: [],
    ci: 'none',
    rounds: { 'developer-1': 0, 'developer-2': 0, 'developer-3': 0 },
    nudged: false,
    unmergeable: 0,
    lastEvent: '',
    lastAt: 0,
  }
}

describe('the status line', () => {
  test('no task, or only merged ones: no line', async () => {
    expect(summary({ tasks: [] })).toBeUndefined()
    expect(summary({ tasks: [task('a', 'merged')] })).toBeUndefined()
  })

  test('the counts by stage, in the order of the stages, joined by a middle dot, with no prefix', async () => {
    const s: State = { tasks: [task('a', 'checking'), task('b', 'developing'), task('c', 'waiting'), task('d', 'developing')] }
    expect(summary(s)).toBe('1 waiting · 2 developing · 1 checking')
  })

  test('merged tasks are left out of the counts', async () => {
    expect(summary({ tasks: [task('a', 'merged'), task('b', 'developing'), task('c', 'merged')] })).toBe('1 developing')
  })

  test('a blocked task is counted', async () => {
    expect(summary({ tasks: [task('a', 'blocked'), task('b', 'liaison')] })).toBe('1 liaison · 1 blocked')
  })
})

describe('the status of a task', () => {
  test('the pull request address comes from the config', async () => {
    const t = { ...task('a', 'checking'), pr: 5 }
    expect(describeTask(t, TEST_CONFIG)).toContain('PR #5 (http://forge.test/acme/widgets/pulls/5)')
  })
})
