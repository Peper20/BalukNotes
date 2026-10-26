// Every call to Forgejo (`tea api`) and to git of the pipeline is here, behind
// the `Forge` interface. The driver and the machine know nothing else about the
// server or the repository; tests use `forge.fake.ts` or a scripted `Run`.
//
// Safety rules kept in this file: it merges only through `merge()` with the head
// commit it was given (the server refuses a moved head), never pushes to `main`,
// never force-pushes, never removes a worktree with `--force`, and touches only
// worktrees under `.claude/worktrees/` of the main checkout.
import type { AfterMerge } from './config'
import type { Ci } from './model'

export type RunResult = { exitCode: number; stdout: string; stderr: string }
/** `$.process.run`: an argument vector, no shell. */
export type Run = (argv: readonly string[], init?: { cwd?: string; timeoutMs?: number }) => Promise<RunResult>

export type PrInfo = {
  state: 'open' | 'closed' | 'merged'
  head: string
  mergeable: boolean
}

export type StatusInfo = { state: Ci; detail: string }

export type ReviewInfo = {
  verdict: 'APPROVED' | 'REQUEST_CHANGES' | 'none'
  /** The commit of the review that decided (the head asked for, or empty). */
  commit: string
  text: string
}

export type MergeOutcome = { ok: true } | { ok: false; kind: 'conflict' | 'moved' | 'other'; message: string }

export type PullOutcome = { pull: 'ok' | 'failed' | 'skipped'; note: string; changed: string[] }

export type Forge = {
  pr(n: number): Promise<PrInfo>
  status(sha: string, pr: number): Promise<StatusInfo>
  review(n: number, head: string): Promise<ReviewInfo>
  merge(n: number, head: string, branch: string): Promise<MergeOutcome>
  comment(n: number, text: string): Promise<void>
  /** Remove the worktrees of agents (with their `worktree-agent-*` branches); the leftovers. */
  removeAgents(ids: readonly string[]): Promise<string[]>
  /** After a merge: the check worktree, the local task branch (only when it is the merged head); the leftovers. */
  cleanupMerged(a: { slug: string; pr: number; head: string }): Promise<string[]>
  /** A blocked task: the local task branch goes only when the server holds the same commit; the leftovers. */
  cleanupBlocked(a: { slug: string; pr?: number }): Promise<string[]>
  pullMain(): Promise<PullOutcome>
  /** The `afterMerge` command of the config, run in the main checkout. */
  install(): Promise<{ ok: boolean; note: string }>
}

export type ForgeOptions = {
  /** The main checkout. */
  root: string
  sleep: (ms: number) => Promise<void>
  /** Waits between tries of a merge the server calls not mergeable yet. */
  mergeWaitMs?: number
  /** The command run in the main checkout after a merge that changed the paths it names. */
  afterMerge?: AfterMerge
}

const MAIN = 'main'
const firstLine = (text: string): string => text.trim().split('\n')[0] ?? ''

function json(r: RunResult, what: string): unknown {
  if (r.exitCode !== 0) throw new Error(`${what}: ${firstLine(r.stderr) || `exit ${r.exitCode}`}`)
  try {
    return JSON.parse(r.stdout)
  } catch {
    throw new Error(`${what}: the answer is not JSON`)
  }
}

const asRecord = (v: unknown): Record<string, unknown> => (typeof v === 'object' && v !== null ? (v as Record<string, unknown>) : {})
const asString = (v: unknown): string => (typeof v === 'string' ? v : '')
const asArray = (v: unknown): unknown[] => (Array.isArray(v) ? v : [])

function toCi(state: string): Ci {
  switch (state) {
    case 'success':
    case 'failure':
    case 'error':
      return state
    // The head did not pass cleanly: a developer's round, not a poll without end.
    case 'warning':
      return 'error'
    default:
      return 'pending'
  }
}

/** The page size of the lists read from the server, and a bound against a server that never ends a list. */
const PAGE_SIZE = 50
const MAX_PAGES = 200

export function makeForge(run: Run, opts: ForgeOptions): Forge {
  const { root, sleep } = opts
  const worktrees = `${root}/.claude/worktrees`
  const sh = (argv: readonly string[], timeoutMs?: number): Promise<RunResult> => run(argv, { cwd: root, timeoutMs })
  const git = (...args: string[]): Promise<RunResult> => sh(['git', ...args])
  const api = (login: string, endpoint: string, ...more: string[]): Promise<RunResult> =>
    sh(['tea', 'api', '--login', login, ...more, endpoint])
  let reviewerLogin: string | undefined

  /** Every item of a list endpoint: pages of `PAGE_SIZE` from 1, until a page is shorter. */
  async function allPages(endpoint: string, what: string): Promise<Record<string, unknown>[]> {
    const items: Record<string, unknown>[] = []
    for (let page = 1; page <= MAX_PAGES; page++) {
      const list = asArray(json(await api('dev', `${endpoint}?limit=${PAGE_SIZE}&page=${page}`), what))
      items.push(...list.map(asRecord))
      if (list.length < PAGE_SIZE) break
    }
    return items
  }

  async function pr(n: number): Promise<PrInfo> {
    const d = asRecord(json(await api('dev', `/repos/{owner}/{repo}/pulls/${n}`), `pull request ${n}`))
    const state = d.merged === true ? 'merged' : asString(d.state) === 'open' ? 'open' : 'closed'
    return { state, head: asString(asRecord(d.head).sha), mergeable: d.mergeable === true }
  }

  async function status(sha: string, n: number): Promise<StatusInfo> {
    const d = asRecord(json(await api('dev', `/repos/{owner}/{repo}/commits/${sha}/status`), `status of ${sha}`))
    const state = toCi(asString(d.state))
    if (state !== 'failure' && state !== 'error') return { state, detail: '' }
    const bad = asArray(d.statuses)
      .map(asRecord)
      .filter(s => asString(s.status) !== 'success' && asString(s.status) !== 'pending')
      .map(s => `- ${asString(s.context)}: ${asString(s.description)} ${asString(s.target_url)}`.trimEnd())
    let comment = ''
    try {
      const comments = await allPages(`/repos/{owner}/{repo}/issues/${n}/comments`, `comments of ${n}`)
      const mine = comments.filter(c => asString(c.body).startsWith(`Full check failed at ${sha}`))
      comment = asString(mine[mine.length - 1]?.body)
    } catch {
      // The statuses are enough.
    }
    const logs = `Step logs: ${worktrees}/check-pr-${n}/tests/.data/check/ (run.log is the whole output).`
    const parts = [comment, bad.length > 0 ? `Failed statuses:\n${bad.join('\n')}` : '', logs].filter(p => p !== '')
    return { state, detail: parts.join('\n\n') }
  }

  async function login(): Promise<string> {
    if (reviewerLogin === undefined) {
      const d = asRecord(json(await api('reviewer', '/user'), 'the reviewer account'))
      reviewerLogin = asString(d.login)
    }
    return reviewerLogin
  }

  async function review(n: number, head: string): Promise<ReviewInfo> {
    const who = await login()
    const list = await allPages(`/repos/{owner}/{repo}/pulls/${n}/reviews`, `reviews of ${n}`)
    const mine = list
      .filter(r => asString(asRecord(r.user).login) === who)
      .filter(r => asString(r.commit_id) === head && r.dismissed !== true && r.stale !== true)
      .filter(r => ['APPROVED', 'REQUEST_CHANGES'].includes(asString(r.state)))
      .sort((a, b) => Number(a.id) - Number(b.id))
    const last = mine[mine.length - 1]
    if (!last) return { verdict: 'none', commit: '', text: '' }
    return {
      verdict: asString(last.state) === 'APPROVED' ? 'APPROVED' : 'REQUEST_CHANGES',
      commit: asString(last.commit_id),
      text: asString(last.body),
    }
  }

  async function deleteRemoteBranch(branch: string): Promise<void> {
    if (branch === MAIN || !branch.startsWith('task/')) return
    const there = await git('ls-remote', '--exit-code', '--heads', 'origin', branch)
    if (there.exitCode === 0) await git('push', 'origin', '--delete', branch)
  }

  async function merge(n: number, head: string, branch: string): Promise<MergeOutcome> {
    let message = ''
    for (let attempt = 0; attempt < 4; attempt++) {
      const now = await pr(n)
      if (now.state === 'merged') {
        await deleteRemoteBranch(branch)
        return { ok: true }
      }
      if (now.state === 'closed') return { ok: false, kind: 'other', message: 'the pull request is closed' }
      if (now.head !== head) return { ok: false, kind: 'moved', message: `the head moved to ${now.head}` }
      const r = await api(
        'owner',
        `/repos/{owner}/{repo}/pulls/${n}/merge`,
        '-X',
        'POST',
        '-f',
        'Do=squash',
        '-f',
        `head_commit_id=${head}`,
        '-F',
        'delete_branch_after_merge=true',
      )
      if (r.exitCode === 0 && (await pr(n)).state === 'merged') {
        await deleteRemoteBranch(branch)
        return { ok: true }
      }
      message = firstLine(r.stderr) || firstLine(r.stdout) || `exit ${r.exitCode}`
      // Right after a push or an approval the server says "not mergeable" while it still checks.
      await sleep(opts.mergeWaitMs ?? 5000)
    }
    const final = await pr(n)
    if (final.head !== head) return { ok: false, kind: 'moved', message }
    return { ok: false, kind: final.mergeable ? 'other' : 'conflict', message }
  }

  async function comment(n: number, text: string): Promise<void> {
    const r = await sh(['tea', 'comment', '--login', 'dev', String(n), text])
    if (r.exitCode !== 0) throw new Error(`comment on ${n}: ${firstLine(r.stderr)}`)
  }

  async function registered(): Promise<Set<string>> {
    const r = await git('worktree', 'list', '--porcelain')
    return new Set(
      r.stdout
        .split('\n')
        .filter(l => l.startsWith('worktree '))
        .map(l => l.slice('worktree '.length)),
    )
  }

  /** `ok` removed or absent; otherwise why not. Never `--force`. */
  async function removeWorktree(path: string): Promise<string | undefined> {
    if (!path.startsWith(`${worktrees}/`) || path.includes('..')) return `refused to touch ${path}`
    if (!(await registered()).has(path)) return undefined
    await git('worktree', 'unlock', path)
    const r = await git('worktree', 'remove', path)
    return r.exitCode === 0 ? undefined : firstLine(r.stderr) || `exit ${r.exitCode}`
  }

  async function localBranch(name: string): Promise<string | undefined> {
    const r = await git('rev-parse', '--verify', '-q', `refs/heads/${name}`)
    return r.exitCode === 0 ? r.stdout.trim() : undefined
  }

  async function removeAgents(ids: readonly string[]): Promise<string[]> {
    const left: string[] = []
    for (const id of ids) {
      if (!/^[\w-]+$/.test(id)) {
        left.push(`agent ${id}: not a plain id`)
        continue
      }
      const why = await removeWorktree(`${worktrees}/agent-${id}`)
      if (why !== undefined) {
        left.push(`worktree agent-${id}: ${why}`)
        continue
      }
      const branch = `worktree-agent-${id}`
      if ((await localBranch(branch)) !== undefined) {
        const r = await git('branch', '-d', branch)
        if (r.exitCode !== 0) left.push(`branch ${branch}: ${firstLine(r.stderr)}`)
      }
    }
    return left
  }

  async function cleanupMerged(a: { slug: string; pr: number; head: string }): Promise<string[]> {
    const left: string[] = []
    const why = await removeWorktree(`${worktrees}/check-pr-${a.pr}`)
    if (why !== undefined) left.push(`worktree check-pr-${a.pr}: ${why}`)
    const branch = `task/${a.slug}`
    const at = await localBranch(branch)
    if (at !== undefined) {
      if (at === a.head) {
        const r = await git('branch', '-D', branch)
        if (r.exitCode !== 0) left.push(`branch ${branch}: ${firstLine(r.stderr)}`)
      } else {
        left.push(`branch ${branch} is at ${at.slice(0, 7)}, not at the merged head ${a.head.slice(0, 7)}: it holds commits that were not merged`)
      }
    }
    return left
  }

  async function cleanupBlocked(a: { slug: string; pr?: number }): Promise<string[]> {
    const left: string[] = []
    const branch = `task/${a.slug}`
    const at = await localBranch(branch)
    if (at === undefined) return left
    let server: string | undefined
    if (a.pr !== undefined) {
      try {
        server = (await pr(a.pr)).head
      } catch {
        server = undefined
      }
    } else {
      const r = await git('ls-remote', '--heads', 'origin', branch)
      server = r.stdout.trim().split(/\s+/)[0] || undefined
    }
    if (server !== undefined && server === at) {
      const r = await git('branch', '-D', branch)
      if (r.exitCode !== 0) left.push(`branch ${branch}: ${firstLine(r.stderr)}`)
    } else {
      left.push(`branch ${branch} at ${at.slice(0, 7)} is kept: the server holds ${server === undefined ? 'nothing' : server.slice(0, 7)}`)
    }
    return left
  }

  async function pullMain(): Promise<PullOutcome> {
    const branch = (await git('rev-parse', '--abbrev-ref', 'HEAD')).stdout.trim()
    if (branch !== MAIN) return { pull: 'skipped', note: `the main checkout is on ${branch || 'no branch'}`, changed: [] }
    const before = (await git('rev-parse', 'HEAD')).stdout.trim()
    const r = await git('pull', '--ff-only')
    if (r.exitCode !== 0) {
      await git('fetch', 'origin')
      return { pull: 'failed', note: firstLine(r.stderr) || firstLine(r.stdout), changed: [] }
    }
    const diff = await git('diff', '--name-only', `${before}..HEAD`)
    return { pull: 'ok', note: '', changed: diff.stdout.split('\n').filter(l => l !== '') }
  }

  async function install(): Promise<{ ok: boolean; note: string }> {
    const { afterMerge } = opts
    if (afterMerge === undefined) return { ok: false, note: 'no afterMerge command in the config' }
    try {
      const r = await sh(afterMerge.command, afterMerge.timeoutSeconds * 1000)
      if (r.exitCode === 0) return { ok: true, note: '' }
      const tail = (r.stderr || r.stdout).trim().split('\n').slice(-3).join(' / ')
      return { ok: false, note: `exit ${r.exitCode}: ${tail}` }
    } catch (err) {
      return { ok: false, note: `${String(err)} (run ${afterMerge.command.join(' ')} by hand)` }
    }
  }

  return { pr, status, review, merge, comment, removeAgents, cleanupMerged, cleanupBlocked, pullMain, install }
}
