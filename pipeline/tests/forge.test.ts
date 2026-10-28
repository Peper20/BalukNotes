// forge.ts against a scripted `Run`: the exact commands, how answers are read,
// and the safety rules. No server, no git.
import { describe, expect, test } from 'claude-code/testing'

import type { AfterMerge } from '../hooks/config'
import { makeForge, type Run, type RunResult } from '../hooks/forge'

const ROOT = '/repo'
const HEAD = 'a'.repeat(40)
const ok = (stdout = ''): RunResult => ({ exitCode: 0, stdout, stderr: '' })
const fail = (stderr: string): RunResult => ({ exitCode: 1, stdout: '', stderr })

/** A scripted machine: the first rule whose text the command line contains answers. */
function script(rules: [string, RunResult | (() => RunResult)][]): { run: Run; log: string[] } {
  const log: string[] = []
  const run: Run = async (argv, init) => {
    expect(init?.cwd).toBe(ROOT)
    const line = argv.join(' ')
    log.push(line)
    for (const [part, answer] of rules) {
      if (line.includes(part)) return typeof answer === 'function' ? answer() : answer
    }
    return ok()
  }
  return { run, log }
}

const write = async (): Promise<void> => undefined
const forgeOf = (run: Run) => makeForge(run, { root: ROOT, sleep: async () => undefined, write, mergeWaitMs: 0 })
const pr = (o: Record<string, unknown>): RunResult => ok(JSON.stringify({ state: 'open', merged: false, mergeable: true, head: { sha: HEAD }, ...o }))

describe('reading the server', () => {
  test('a pull request, through tea as dev', async () => {
    const s = script([['pulls/7', pr({})]])
    const info = await forgeOf(s.run).pr(7)
    expect(info).toEqual({ state: 'open', head: HEAD, mergeable: true })
    expect(s.log[0]).toBe('tea api --login dev /repos/{owner}/{repo}/pulls/7')
    const m = script([['pulls/7', pr({ merged: true, state: 'closed' })]])
    expect((await forgeOf(m.run).pr(7)).state).toBe('merged')
    const c = script([['pulls/7', pr({ state: 'closed' })]])
    expect((await forgeOf(c.run).pr(7)).state).toBe('closed')
  })

  test('the combined status; a failure carries the check comment and the logs path', async () => {
    const body = `Full check failed at ${HEAD}: clippy.\n\n\`\`\`\nstep result\n\`\`\``
    const s = script([
      ['/status', ok(JSON.stringify({ state: 'failure', statuses: [{ context: 'ci', status: 'failure', description: 'clippy failed', target_url: 'http://x/1' }] }))],
      ['/comments', ok(JSON.stringify([{ body: 'Full check failed at old: fmt.' }, { body }]))],
    ])
    const st = await forgeOf(s.run).status(HEAD, 7)
    expect(st.state).toBe('failure')
    expect(st.detail).toContain('Full check failed at aaaa')
    expect(st.detail).not.toContain('at old')
    expect(st.detail).toContain('ci: clippy failed http://x/1')
    expect(st.detail).toContain('/repo/.claude/worktrees/check-pr-7/.pipeline/check/')
    const custom = script([['/status', ok(JSON.stringify({ state: 'failure', statuses: [] }))], ['/comments', ok('[]')]])
    const withLogs = makeForge(custom.run, { root: ROOT, sleep: async () => undefined, write, checkLogs: 'out/check' })
    expect((await withLogs.status(HEAD, 7)).detail).toContain('/repo/.claude/worktrees/check-pr-7/out/check/')
    for (const [state, want] of [['success', 'success'], ['pending', 'pending'], ['', 'pending'], ['unknown', 'pending'], ['error', 'error'], ['warning', 'error']] as const) {
      const t = script([['/status', ok(JSON.stringify({ state, statuses: [] }))], ['/comments', ok('[]')]])
      expect((await forgeOf(t.run).status(HEAD, 7)).state).toBe(want)
    }
  })

  test('a combined status warning is an error for the developer, with the statuses that did not pass', async () => {
    const statuses = [
      { context: 'ci', status: 'warning', description: 'slow', target_url: 'http://x/2' },
      { context: 'lint', status: 'success', description: 'ok', target_url: '' },
      { context: 'wait', status: 'pending', description: '', target_url: '' },
    ]
    const s = script([['/status', ok(JSON.stringify({ state: 'warning', statuses }))], ['/comments', ok('[]')]])
    const st = await forgeOf(s.run).status(HEAD, 7)
    expect(st.state).toBe('error')
    expect(st.detail).toContain('ci: slow http://x/2')
    expect(st.detail).not.toContain('lint')
    expect(st.detail).not.toContain('wait')
  })

  test('comments are read page by page: a failure comment on page 2 is found', async () => {
    const filler = Array.from({ length: 50 }, (_, i) => ({ body: `chatter ${i}` }))
    const body = `Full check failed at ${HEAD}: tests.`
    const s = script([
      ['/status', ok(JSON.stringify({ state: 'failure', statuses: [] }))],
      ['/comments?limit=50&page=2', ok(JSON.stringify([{ body: 'later chatter' }, { body }]))],
      ['/comments?limit=50&page=1', ok(JSON.stringify(filler))],
    ])
    const st = await forgeOf(s.run).status(HEAD, 7)
    expect(st.detail).toContain(body)
    expect(s.log.filter(l => l.includes('/comments'))).toHaveLength(2)
  })

  test('the reviews are read page by page: the last review on page 2 decides', async () => {
    const mine = (id: number, state: string) => ({ id, user: { login: 'agent-reviewer' }, state, commit_id: HEAD, body: `review ${id}` })
    const page1 = Array.from({ length: 50 }, (_, i) => mine(i + 1, 'REQUEST_CHANGES'))
    const s = script([
      ['--login reviewer /user', ok('{"login":"agent-reviewer"}')],
      ['/reviews?limit=50&page=2', ok(JSON.stringify([mine(51, 'APPROVED')]))],
      ['/reviews?limit=50&page=1', ok(JSON.stringify(page1))],
    ])
    expect(await forgeOf(s.run).review(7, HEAD)).toEqual({ verdict: 'APPROVED', commit: HEAD, text: 'review 51' })
  })

  test('the review: the reviewer account, this commit, not stale, the latest one', async () => {
    const reviews = [
      { id: 1, user: { login: 'agent-reviewer' }, state: 'REQUEST_CHANGES', commit_id: HEAD, body: 'first' },
      { id: 2, user: { login: 'agent-reviewer' }, state: 'APPROVED', commit_id: 'b'.repeat(40), body: 'older commit' },
      { id: 3, user: { login: 'someone' }, state: 'APPROVED', commit_id: HEAD, body: 'not the reviewer' },
      { id: 4, user: { login: 'agent-reviewer' }, state: 'APPROVED', commit_id: HEAD, body: 'stale', stale: true },
      { id: 5, user: { login: 'agent-reviewer' }, state: 'COMMENT', commit_id: HEAD, body: 'a comment' },
      { id: 6, user: { login: 'agent-reviewer' }, state: 'APPROVED', commit_id: HEAD, body: 'dismissed', dismissed: true },
    ]
    const s = script([['--login reviewer /user', ok('{"login":"agent-reviewer"}')], ['/reviews', () => ok(JSON.stringify(reviews))]])
    const f = forgeOf(s.run)
    expect(await f.review(7, HEAD)).toEqual({ verdict: 'REQUEST_CHANGES', commit: HEAD, text: 'first' })
    reviews.push({ id: 7, user: { login: 'agent-reviewer' }, state: 'APPROVED', commit_id: HEAD, body: 'ok now' })
    expect((await f.review(7, HEAD)).verdict).toBe('APPROVED')
    expect(await f.review(7, 'c'.repeat(40))).toEqual({ verdict: 'none', commit: '', text: '' })
  })
})

describe('merging', () => {
  test('squash as owner at the given head; the remote branch goes', async () => {
    let merged = false
    const s = script([
      ['pulls/7/merge', () => ((merged = true), ok())],
      ['pulls/7', () => pr({ merged, state: merged ? 'closed' : 'open' })],
      ['ls-remote', ok(`${HEAD}\trefs/heads/task/feat`)],
    ])
    const out = await forgeOf(s.run).merge(7, HEAD, 'task/feat')
    expect(out).toEqual({ ok: true })
    const call = s.log.find(l => l.includes('/merge')) ?? ''
    expect(call).toContain('--login owner')
    expect(call).toContain('-X POST')
    expect(call).toContain('Do=squash')
    expect(call).toContain(`head_commit_id=${HEAD}`)
    expect(s.log).toContain('git push origin --delete task/feat')
  })

  test('a moved head is not merged', async () => {
    const s = script([['pulls/7', pr({ head: { sha: 'b'.repeat(40) } })]])
    const out = await forgeOf(s.run).merge(7, HEAD, 'task/feat')
    expect(out.ok).toBe(false)
    expect(!out.ok && out.kind).toBe('moved')
    expect(s.log.some(l => l.includes('/merge'))).toBe(false)
  })

  test('a refused merge on a conflicting pull request is a conflict, on a mergeable one other', async () => {
    for (const [mergeable, kind] of [[false, 'conflict'], [true, 'other']] as const) {
      const s = script([['pulls/7/merge', fail('409 conflict')], ['pulls/7', pr({ mergeable })]])
      const out = await forgeOf(s.run).merge(7, HEAD, 'task/feat')
      expect(!out.ok && out.kind).toBe(kind)
      // It tried more than once: the server may still be checking.
      expect(s.log.filter(l => l.includes('/merge'))).toHaveLength(4)
    }
  })

  test('a pull request that is merged already counts as merged', async () => {
    const s = script([['pulls/7', pr({ merged: true })], ['ls-remote', fail('')]])
    expect(await forgeOf(s.run).merge(7, HEAD, 'task/feat')).toEqual({ ok: true })
    expect(s.log.some(l => l.includes('/merge'))).toBe(false)
  })

  test('never pushes main, never forces', async () => {
    const s = script([['pulls/7', pr({ merged: true })], ['ls-remote', ok(`${HEAD}\trefs/heads/main`)]])
    await forgeOf(s.run).merge(7, HEAD, 'main')
    await forgeOf(s.run).merge(7, HEAD, 'other/branch')
    expect(s.log.some(l => l.startsWith('git push'))).toBe(false)
  })
})

describe('git: cleanup', () => {
  const agentWt = `${ROOT}/.claude/worktrees/agent-abc123`
  const porcelain = `worktree ${ROOT}\nHEAD x\nbranch refs/heads/main\n\nworktree ${agentWt}\nHEAD y\nbranch refs/heads/task/feat\n`

  test('an agent: unlock, remove without force, then its branch with -d', async () => {
    const s = script([
      ['worktree list', ok(porcelain)],
      ['rev-parse --verify -q refs/heads/worktree-agent-abc123', ok(`${HEAD}\n`)],
    ])
    expect(await forgeOf(s.run).removeAgents(['abc123'])).toEqual([])
    expect(s.log).toContain(`git worktree unlock ${agentWt}`)
    expect(s.log).toContain(`git worktree remove ${agentWt}`)
    expect(s.log).toContain('git branch -d worktree-agent-abc123')
    expect(s.log.some(l => l.includes('--force') || l.includes(' -f') || l.includes('branch -D'))).toBe(false)
    expect(s.log.indexOf(`git worktree remove ${agentWt}`)).toBeLessThan(s.log.indexOf('git branch -d worktree-agent-abc123'))
  })

  test('a worktree git refuses to remove stays, with the reason, and its branch is kept', async () => {
    const s = script([
      ['worktree list', ok(porcelain)],
      ['worktree remove', fail('fatal: contains modified or untracked files, use --force to delete it')],
    ])
    const left = await forgeOf(s.run).removeAgents(['abc123'])
    expect(left).toHaveLength(1)
    expect(left[0]).toContain('agent-abc123')
    expect(left[0]).toContain('untracked files')
    expect(s.log.some(l => l.startsWith('git branch'))).toBe(false)
  })

  test('a worktree that is gone is nothing to clean', async () => {
    const s = script([['worktree list', ok(`worktree ${ROOT}\n`)], ['rev-parse --verify', fail('')]])
    expect(await forgeOf(s.run).removeAgents(['abc123'])).toEqual([])
    expect(s.log.some(l => l.includes('worktree remove'))).toBe(false)
  })

  test('ids that are not plain are not turned into paths', async () => {
    const s = script([])
    const left = await forgeOf(s.run).removeAgents(['../../etc', 'a b'])
    expect(left).toHaveLength(2)
    expect(s.log).toEqual([])
  })

  test('a merged task: the check worktree, and task/<slug> only when it is the merged head', async () => {
    const check = `${ROOT}/.claude/worktrees/check-pr-7`
    const porcelainCheck = `worktree ${ROOT}\n\nworktree ${check}\nHEAD z\ndetached\n`
    const same = script([['worktree list', ok(porcelainCheck)], ['refs/heads/task/feat', ok(`${HEAD}\n`)]])
    expect(await forgeOf(same.run).cleanupMerged({ slug: 'feat', pr: 7, head: HEAD })).toEqual([])
    expect(same.log).toContain(`git worktree remove ${check}`)
    expect(same.log).toContain('git branch -D task/feat')
    const other = script([['worktree list', ok('')], ['refs/heads/task/feat', ok(`${'b'.repeat(40)}\n`)]])
    const left = await forgeOf(other.run).cleanupMerged({ slug: 'feat', pr: 7, head: HEAD })
    expect(left[0]).toContain('not merged')
    expect(other.log.some(l => l.includes('branch -D'))).toBe(false)
  })

  test('a blocked task: the local branch goes only when the server holds the same commit', async () => {
    const same = script([['refs/heads/task/feat', ok(`${HEAD}\n`)], ['pulls/7', pr({})]])
    expect(await forgeOf(same.run).cleanupBlocked({ slug: 'feat', pr: 7 })).toEqual([])
    expect(same.log).toContain('git branch -D task/feat')
    const differs = script([['refs/heads/task/feat', ok(`${'b'.repeat(40)}\n`)], ['pulls/7', pr({})]])
    const left = await forgeOf(differs.run).cleanupBlocked({ slug: 'feat', pr: 7 })
    expect(left[0]).toContain('kept')
    expect(differs.log.some(l => l.includes('branch -D'))).toBe(false)
    const nopr = script([['refs/heads/task/feat', ok(`${HEAD}\n`)], ['ls-remote', ok(`${HEAD}\trefs/heads/task/feat\n`)]])
    expect(await forgeOf(nopr.run).cleanupBlocked({ slug: 'feat' })).toEqual([])
  })
})

describe('git: main checkout', () => {
  test('pull --ff-only on main, the changed files are listed', async () => {
    const s = script([
      ['rev-parse --abbrev-ref HEAD', ok('main\n')],
      ['rev-parse HEAD', ok('old\n')],
      ['diff --name-only old..HEAD', ok('src/a.rs\ndocs/b.md\n')],
    ])
    const out = await forgeOf(s.run).pullMain()
    expect(out).toEqual({ pull: 'ok', note: '', changed: ['src/a.rs', 'docs/b.md'] })
    expect(s.log).toContain('git pull --ff-only')
  })

  test('not on main: skipped; a failed pull fetches and says so', async () => {
    const s = script([['rev-parse --abbrev-ref HEAD', ok('feature\n')]])
    expect((await forgeOf(s.run).pullMain()).pull).toBe('skipped')
    expect(s.log.some(l => l.includes('pull'))).toBe(false)
    const f = script([['rev-parse --abbrev-ref HEAD', ok('main\n')], ['pull --ff-only', fail('fatal: Not possible to fast-forward')]])
    const out = await forgeOf(f.run).pullMain()
    expect(out.pull).toBe('failed')
    expect(f.log).toContain('git fetch origin')
  })

  describe('the after-merge command of the config', () => {
    const afterMerge: AfterMerge = { paths: ['src/'], command: ['scripts/after.sh', '--quick'], timeoutSeconds: 90 }
    const withConfig = (run: Run) => makeForge(run, { root: ROOT, sleep: async () => undefined, write, afterMerge })

    test('it runs in the main checkout with the timeout of the config', async () => {
      const calls: { argv: readonly string[]; timeoutMs?: number }[] = []
      const run: Run = async (argv, init) => {
        calls.push({ argv, timeoutMs: init?.timeoutMs })
        return ok()
      }
      expect(await withConfig(run).install()).toEqual({ ok: true, note: '' })
      expect(calls).toEqual([{ argv: ['scripts/after.sh', '--quick'], timeoutMs: 90_000 }])
    })

    test('a failure carries the exit code and the tail of the output; a command that cannot run says how to run it by hand', async () => {
      const failed = await withConfig(async () => ({ exitCode: 3, stdout: '', stderr: 'a\nb\nc\nd\n' })).install()
      expect(failed).toEqual({ ok: false, note: 'exit 3: b / c / d' })
      const thrown = await withConfig(async () => {
        throw new Error('timeout')
      }).install()
      expect(thrown.ok).toBe(false)
      expect(thrown.note).toContain('run scripts/after.sh --quick by hand')
    })

    test('no command in the config: nothing runs', async () => {
      const s = script([])
      expect((await forgeOf(s.run).install()).ok).toBe(false)
      expect(s.log).toEqual([])
    })
  })
})

describe('a check that did not finish', () => {
  const failing = ['/status', ok(JSON.stringify({ state: 'failure', statuses: [] }))] as [string, RunResult]

  test('a failure is finished only with the comment of the check for this head', async () => {
    const commented = script([failing, ['/comments', ok(JSON.stringify([{ body: `Full check failed at ${HEAD}: clippy.\n\nlog` }]))]])
    expect((await forgeOf(commented.run).status(HEAD, 7)).finished).toBe(true)
    const other = script([failing, ['/comments', ok(JSON.stringify([{ body: `Full check failed at ${'b'.repeat(40)}: clippy.` }, { body: `Not the first line\nFull check failed at ${HEAD}` }]))]])
    expect((await forgeOf(other.run).status(HEAD, 7)).finished).toBe(false)
    const unreadable = script([failing, ['/comments', fail('boom')]])
    expect((await forgeOf(unreadable.run).status(HEAD, 7)).finished).toBe(false)
    const passed = script([['/status', ok(JSON.stringify({ state: 'success', statuses: [] }))]])
    expect((await forgeOf(passed.run).status(HEAD, 7)).finished).toBe(true)
  })

  test('rerun: the newest actions run of the head is re-run', async () => {
    const s = script([['head_sha=', ok(JSON.stringify({ workflow_runs: [{ id: 31 }, { id: 36 }, { id: 33 }] }))]])
    await forgeOf(s.run).rerun(HEAD)
    expect(s.log).toEqual([
      `tea api --login dev /repos/{owner}/{repo}/actions/runs?head_sha=${HEAD}&limit=50`,
      'tea api --login dev -X POST /repos/{owner}/{repo}/actions/runs/36/rerun',
    ])
  })

  test('rerun: no run, or an error answer of a tea that exits 0, is an error', async () => {
    const none = script([['head_sha=', ok(JSON.stringify({ workflow_runs: [] }))]])
    await expect(forgeOf(none.run).rerun(HEAD)).rejects.toThrow('no actions run')
    const running = script([
      ['head_sha=', ok(JSON.stringify({ workflow_runs: [{ id: 5 }] }))],
      ['/rerun', ok('{"message":"the run is not finished","url":"x"}')],
    ])
    await expect(forgeOf(running.run).rerun(HEAD)).rejects.toThrow('run 5: the run is not finished')
  })
})

describe('a dead developer', () => {
  const WT = `${ROOT}/.claude/worktrees/agent-abc123`
  const porcelain = `worktree ${ROOT}\n\nworktree ${WT}\nHEAD z\nbranch refs/heads/task/feat\n`

  /** Commands run in the worktree answer by `inWt`, the others by `rootRules` (a worktree list first). */
  function salvaging(inWt: [string, RunResult][], rootRules: [string, RunResult][] = []) {
    const log: string[] = []
    const written: { path: string; text: string }[] = []
    const run: Run = async (argv, init) => {
      const line = argv.join(' ')
      log.push(`${init?.cwd === WT ? 'wt' : 'root'}: ${line}`)
      const rules = init?.cwd === WT ? inWt : [['worktree list', ok(porcelain)] as [string, RunResult], ...rootRules]
      for (const [part, answer] of rules) if (line.includes(part)) return answer
      return ok()
    }
    const forge = makeForge(run, { root: ROOT, sleep: async () => undefined, write: async (path, text) => void written.push({ path, text }) })
    return { forge, log, written }
  }

  test('the changes and the untracked files go to a patch, then the worktree is removed with --force; the branch stays', async () => {
    const s = salvaging([
      ['diff HEAD --binary', ok('diff --git a/a b/a\n+x\n')],
      ['ls-files', ok('new.txt\0dir/other.txt\0')],
      ['--no-index --binary -- /dev/null new.txt', { exitCode: 1, stdout: 'diff --git a/new.txt b/new.txt\nnew file', stderr: '' }],
      ['--no-index --binary -- /dev/null dir/other.txt', { exitCode: 1, stdout: 'diff --git a/dir/other.txt b/dir/other.txt\n', stderr: '' }],
    ])
    const out = await s.forge.salvage({ slug: 'feat', agentId: 'abc123' })
    const path = `${ROOT}/.claude/worktrees/salvage/feat-abc123.patch`
    expect(out).toEqual({ patch: path })
    expect(s.written).toHaveLength(1)
    expect(s.written[0]?.path).toBe(path)
    expect(s.written[0]?.text).toBe('diff --git a/a b/a\n+x\ndiff --git a/new.txt b/new.txt\nnew file\ndiff --git a/dir/other.txt b/dir/other.txt\n')
    expect(s.log).toContain(`root: git worktree remove --force ${WT}`)
    expect(s.log.some(l => l.includes('branch -'))).toBe(false)
    // The patch is written before the worktree goes.
    expect(s.log.findIndex(l => l.includes('mkdir'))).toBeLessThan(s.log.findIndex(l => l.includes('remove --force')))
  })

  test('nothing to save: no patch, the worktree is still freed', async () => {
    const s = salvaging([])
    expect(await s.forge.salvage({ slug: 'feat', agentId: 'abc123' })).toEqual({})
    expect(s.written).toEqual([])
    expect(s.log).toContain(`root: git worktree remove --force ${WT}`)
  })

  test('a patch that cannot be made keeps the worktree', async () => {
    const s = salvaging([['diff HEAD --binary', fail('fatal: bad index')]])
    const out = await s.forge.salvage({ slug: 'feat', agentId: 'abc123' })
    expect(out.patch).toBeUndefined()
    expect(out.note).toContain('fatal: bad index')
    expect(s.log.some(l => l.includes('worktree remove'))).toBe(false)
  })

  test('a worktree that is gone, or an id that is not plain, is nothing to save', async () => {
    const gone = script([['worktree list', ok(`worktree ${ROOT}\n`)]])
    expect(await forgeOf(gone.run).salvage({ slug: 'feat', agentId: 'abc123' })).toEqual({})
    const bad = script([])
    expect((await forgeOf(bad.run).salvage({ slug: 'feat', agentId: '../x' })).note).toContain('not a plain id')
    expect(bad.log).toEqual([])
  })

  test('worktrees of agents and checks that nothing references are listed', async () => {
    const wt = (n: string) => `worktree ${ROOT}/.claude/worktrees/${n}\nHEAD z\n\n`
    const s = script([['worktree list', ok(`worktree ${ROOT}\n\n${wt('agent-live')}${wt('agent-lost')}${wt('check-pr-7')}${wt('check-pr-8')}${wt('other')}`)]])
    const found = await forgeOf(s.run).unreferenced({ agents: ['live'], prs: [7] })
    expect(found).toEqual([`${ROOT}/.claude/worktrees/agent-lost`, `${ROOT}/.claude/worktrees/check-pr-8`])
  })
})
