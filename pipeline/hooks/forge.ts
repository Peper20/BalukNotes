// Every call to Forgejo (`tea api`) and to git of the pipeline is here, behind
// the `Forge` interface. The driver and the machine know nothing else about the
// server or the repository; tests use `forge.fake.ts` or a scripted `Run`.
//
// Safety rules kept in this file: it merges only through `merge()` with the head
// commit it was given (the server refuses a moved head), never pushes to `main`,
// never force-pushes, removes a worktree with `--force` only in `salvage`, after
// the unsaved work in it is written to a patch, and touches only worktrees under
// `.claude/worktrees/` of the main checkout.
import { type AfterMerge, DEFAULT_CHECK_LOGS } from './config'
import type { Ci } from './model'

export type RunResult = { exitCode: number; stdout: string; stderr: string }
/** `$.process.run`: an argument vector, no shell. */
export type Run = (argv: readonly string[], init?: { cwd?: string; timeoutMs?: number }) => Promise<RunResult>

export type PrInfo = {
  state: 'open' | 'closed' | 'merged'
  head: string
  mergeable: boolean
}

export type StatusInfo = {
  state: Ci
  detail: string
  /** The check ran to its end: not a failure without the comment `check-pr.sh` posts, which a cancelled or killed job leaves. */
  finished: boolean
}

export type ReviewInfo = {
  verdict: 'APPROVED' | 'REQUEST_CHANGES' | 'none'
  /** The commit of the review that decided (the head asked for, or empty). */
  commit: string
  text: string
}

export type MergeOutcome = { ok: true } | { ok: false; kind: 'conflict' | 'moved' | 'other'; message: string }

export type SalvageOutcome = { patch?: string; note?: string }

/** `timedOut`: the command ran past its time limit (it waited for the lock of heavy work behind a check) and was cut. */
export type InstallOutcome = { ok: boolean; note: string; timedOut?: boolean }

export type PullOutcome = { pull: 'ok' | 'failed' | 'skipped'; note: string; changed: string[] }

export type Forge = {
  pr(n: number): Promise<PrInfo>
  status(sha: string, pr: number): Promise<StatusInfo>
  review(n: number, head: string): Promise<ReviewInfo>
  /** Run the full check of this head again (the actions run of the head); throws why not. */
  rerun(sha: string): Promise<void>
  merge(n: number, head: string, branch: string): Promise<MergeOutcome>
  comment(n: number, text: string): Promise<void>
  /** Remove the worktrees of agents (with their `worktree-agent-*` branches); the leftovers. */
  removeAgents(ids: readonly string[]): Promise<string[]>
  /** After a merge: the check worktree, the local task branch (only when it is the merged head); the leftovers. */
  cleanupMerged(a: { slug: string; pr: number; head: string }): Promise<string[]>
  /** A blocked task: the local task branch goes only when the server holds the same commit; the leftovers. */
  cleanupBlocked(a: { slug: string; pr?: number }): Promise<string[]>
  /**
   * A developer that is gone: its unsaved changes go to a patch under `.claude/worktrees/salvage/`,
   * its worktree is removed (its branch stays). No patch for no changes; `note` says what went wrong.
   */
  salvage(a: { slug: string; agentId: string }): Promise<SalvageOutcome>
  /** Worktrees `agent-*` and `check-pr-*` that are not the agents' and pull requests' given. */
  unreferenced(a: { agents: readonly string[]; prs: readonly number[] }): Promise<string[]>
  pullMain(): Promise<PullOutcome>
  /** The `afterMerge` command of the config, run in the main checkout. */
  install(): Promise<InstallOutcome>
}

export type ForgeOptions = {
  /** The main checkout. */
  root: string
  sleep: (ms: number) => Promise<void>
  /** Waits between tries of a merge the server calls not mergeable yet. */
  mergeWaitMs?: number
  /** The command run in the main checkout after a merge that changed the paths it names. */
  afterMerge?: AfterMerge
  /** Writes a text file (`$.fs.write`). */
  write: (path: string, text: string) => Promise<void>
  /** The folder of the check logs in a checkout (`check.logs` of the config). */
  checkLogs?: string
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

function safeParse(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return undefined
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
  const checkLogs = opts.checkLogs ?? DEFAULT_CHECK_LOGS
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
    if (state !== 'failure' && state !== 'error') return { state, detail: '', finished: true }
    const bad = asArray(d.statuses)
      .map(asRecord)
      .filter(s => asString(s.status) !== 'success' && asString(s.status) !== 'pending')
      .map(s => `- ${asString(s.context)}: ${asString(s.description)} ${asString(s.target_url)}`.trimEnd())
    let comment = ''
    try {
      const comments = await allPages(`/repos/{owner}/{repo}/issues/${n}/comments`, `comments of ${n}`)
      const mine = comments.filter(c => firstLine(asString(c.body)).startsWith(`Full check failed at ${sha}`))
      comment = asString(mine[mine.length - 1]?.body)
    } catch {
      // The statuses are enough; without the comment the check counts as not finished.
    }
    const logs = `Step logs: ${worktrees}/check-pr-${n}/${checkLogs}/ (run.log is the whole output).`
    const parts = [comment, bad.length > 0 ? `Failed statuses:\n${bad.join('\n')}` : '', logs].filter(p => p !== '')
    return { state, detail: parts.join('\n\n'), finished: comment !== '' }
  }

  async function rerun(sha: string): Promise<void> {
    const d = asRecord(json(await api('dev', `/repos/{owner}/{repo}/actions/runs?head_sha=${sha}&limit=50`), `runs of ${sha}`))
    const ids = asArray(d.workflow_runs).map(r => Number(asRecord(r).id))
    if (ids.length === 0) throw new Error(`no actions run for ${sha}`)
    const id = Math.max(...ids)
    // `tea api` exits 0 on an error answer: success is the empty 204, a failure has a JSON message.
    const r = await api('dev', `/repos/{owner}/{repo}/actions/runs/${id}/rerun`, '-X', 'POST')
    const message = asString(asRecord(safeParse(r.stdout)).message)
    if (r.exitCode !== 0 || message !== '') throw new Error(`re-run of run ${id}: ${message || firstLine(r.stderr) || `exit ${r.exitCode}`}`)
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

  /** A patch of everything the worktree holds that `HEAD` does not: tracked changes and untracked files. */
  async function unsavedWork(wt: string): Promise<string> {
    const inWt = (...argv: string[]): Promise<RunResult> => run(argv, { cwd: wt })
    const diff = await inWt('git', 'diff', 'HEAD', '--binary')
    if (diff.exitCode !== 0) throw new Error(`git diff in ${wt}: ${firstLine(diff.stderr) || `exit ${diff.exitCode}`}`)
    const parts = [diff.stdout]
    const listed = await inWt('git', 'ls-files', '--others', '--exclude-standard', '-z')
    if (listed.exitCode !== 0) throw new Error(`git ls-files in ${wt}: ${firstLine(listed.stderr) || `exit ${listed.exitCode}`}`)
    for (const file of listed.stdout.split('\0').filter(f => f !== '')) {
      // Exit 1 is "differs", which a new file always does.
      const added = await inWt('git', 'diff', '--no-index', '--binary', '--', '/dev/null', file)
      if (added.exitCode > 1) throw new Error(`git diff of ${file} in ${wt}: ${firstLine(added.stderr) || `exit ${added.exitCode}`}`)
      parts.push(added.stdout)
    }
    return parts.map(p => (p === '' || p.endsWith('\n') ? p : `${p}\n`)).join('')
  }

  async function salvage(a: { slug: string; agentId: string }): Promise<SalvageOutcome> {
    if (!/^[\w-]+$/.test(a.agentId) || !/^[\w.-]+$/.test(a.slug)) return { note: `agent ${a.agentId}: not a plain id` }
    const wt = `${worktrees}/agent-${a.agentId}`
    if (!(await registered()).has(wt)) return {}
    let patch: string | undefined
    try {
      const text = await unsavedWork(wt)
      if (text.trim() !== '') {
        patch = `${worktrees}/salvage/${a.slug}-${a.agentId}.patch`
        await run(['mkdir', '-p', `${worktrees}/salvage`], { cwd: root })
        await opts.write(patch, text)
      }
    } catch (err) {
      return { note: `${err instanceof Error ? err.message : String(err)}; the worktree is kept` }
    }
    await git('worktree', 'unlock', wt)
    const r = await git('worktree', 'remove', '--force', wt)
    if (r.exitCode !== 0) return { patch, note: `worktree ${wt}: ${firstLine(r.stderr) || `exit ${r.exitCode}`}` }
    return { patch }
  }

  async function unreferenced(a: { agents: readonly string[]; prs: readonly number[] }): Promise<string[]> {
    const found: string[] = []
    for (const path of [...(await registered())].filter(p => p.startsWith(`${worktrees}/`)).sort()) {
      const name = path.slice(worktrees.length + 1)
      if (name.startsWith('agent-') && !a.agents.includes(name.slice('agent-'.length))) found.push(path)
      const pr = /^check-pr-(\d+)$/.exec(name)
      if (pr && !a.prs.includes(Number(pr[1]))) found.push(path)
    }
    return found
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

  async function install(): Promise<InstallOutcome> {
    const { afterMerge } = opts
    if (afterMerge === undefined) return { ok: false, note: 'no afterMerge command in the config' }
    try {
      const r = await sh(afterMerge.command, afterMerge.timeoutSeconds * 1000)
      if (r.exitCode === 0) return { ok: true, note: '' }
      const tail = (r.stderr || r.stdout).trim().split('\n').slice(-3).join(' / ')
      return { ok: false, note: `exit ${r.exitCode}: ${tail}` }
    } catch (err) {
      // `$.process.run` rejects both when the command cannot start and when the time limit cut it ("still running after <ms>ms").
      if (/still running after|time(d)?[ -]?out/i.test(String(err))) return { ok: false, note: 'timed out', timedOut: true }
      return { ok: false, note: `${String(err)} (run ${afterMerge.command.join(' ')} by hand)` }
    }
  }

  return { pr, status, review, rerun, merge, comment, removeAgents, salvage, unreferenced, cleanupMerged, cleanupBlocked, pullMain, install }
}
