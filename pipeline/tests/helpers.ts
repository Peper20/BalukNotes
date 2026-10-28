// Fakes for the tests: a server and a repository in memory (the test double of
// forge.ts) and the ports of the driver. No test talks to the real server.
import type { Config } from '../hooks/config'
import type { Input, Ports } from '../hooks/driver'
import type { Forge, MergeOutcome, PrInfo, PullOutcome, ReviewInfo, SalvageOutcome, StatusInfo } from '../hooks/forge'
import { emptyState, step } from '../hooks/machine'
import type { Action, Event, Result, State, Task } from '../hooks/model'

/** A project layer for the tests: every key set. */
export const TEST_CONFIG: Config = {
  project: 'widgets',
  guide: 'docs/pipeline.md',
  map: 'docs/rules.md',
  forge: { url: 'http://forge.test', repo: 'acme/widgets' },
  afterMerge: { paths: ['src/', 'lib/'], command: ['scripts/after.sh'], timeoutSeconds: 600 },
  targetedTests: 'make test-one T=<name>',
}

export class FakeServer {
  prs = new Map<number, PrInfo>()
  statuses = new Map<string, StatusInfo>()
  reviews = new Map<number, { head: string; verdict: 'APPROVED' | 'REQUEST_CHANGES'; text: string }[]>()
  /** Calls of the Forge interface, in order, as readable lines. */
  calls: string[] = []
  mergeFails: MergeOutcome | undefined
  /** `merge` throws this text, as forge.ts does when `tea` fails. */
  mergeThrows: string | undefined
  pull: PullOutcome = { pull: 'ok', note: '', changed: [] }
  installOk = true
  /** The next installs run past their time limit (the lock of heavy work was held), this many. */
  installTimeouts = 0
  /** Left over by the cleanup, per call kind. */
  leftovers: string[] = []
  /** The pull request becomes mergeable=false for these numbers. */
  conflicting = new Set<number>()
  /** `rerun` throws this text. */
  rerunThrows: string | undefined
  /** What `salvage` answers, per agent id. */
  salvaged = new Map<string, SalvageOutcome>()
  /** What `unreferenced` answers. */
  orphans: string[] = []

  open(n: number, head: string): void {
    this.prs.set(n, { state: 'open', head, mergeable: true })
  }
  setStatus(sha: string, state: StatusInfo['state'], detail = '', finished = true): void {
    this.statuses.set(sha, { state, detail, finished })
  }
  review(n: number, head: string, verdict: 'APPROVED' | 'REQUEST_CHANGES', text = ''): void {
    this.reviews.set(n, [...(this.reviews.get(n) ?? []), { head, verdict, text }])
  }
  push(n: number, head: string): void {
    const p = this.prs.get(n)
    if (p) this.prs.set(n, { ...p, head })
  }

  forge(): Forge {
    const calls = this.calls
    return {
      pr: async n => {
        const p = this.prs.get(n)
        if (!p) throw new Error(`no pull request ${n}`)
        return { ...p, mergeable: p.mergeable && !this.conflicting.has(n) }
      },
      status: async sha => this.statuses.get(sha) ?? { state: 'pending', detail: '', finished: true },
      rerun: async sha => {
        calls.push(`rerun ${sha}`)
        if (this.rerunThrows !== undefined) throw new Error(this.rerunThrows)
      },
      salvage: async a => {
        calls.push(`salvage ${a.slug} ${a.agentId}`)
        return this.salvaged.get(a.agentId) ?? {}
      },
      unreferenced: async a => {
        return [...this.orphans]
      },
      review: async (n, head): Promise<ReviewInfo> => {
        const mine = (this.reviews.get(n) ?? []).filter(r => r.head === head)
        const last = mine[mine.length - 1]
        return last ? { verdict: last.verdict, commit: last.head, text: last.text } : { verdict: 'none', commit: '', text: '' }
      },
      merge: async (n, head, branch) => {
        calls.push(`merge ${n} ${head} ${branch}`)
        if (this.mergeThrows !== undefined) throw new Error(this.mergeThrows)
        if (this.mergeFails) return this.mergeFails
        const p = this.prs.get(n)
        if (!p || p.head !== head) return { ok: false, kind: 'moved', message: 'moved' }
        this.prs.set(n, { ...p, state: 'merged' })
        return { ok: true }
      },
      comment: async (n, text) => {
        calls.push(`comment ${n} ${text.split('\n')[0]}`)
      },
      removeAgents: async ids => {
        calls.push(`removeAgents ${ids.join(',')}`)
        return [...this.leftovers]
      },
      cleanupMerged: async a => {
        calls.push(`cleanupMerged ${a.slug} ${a.pr}`)
        return []
      },
      cleanupBlocked: async a => {
        calls.push(`cleanupBlocked ${a.slug}`)
        return []
      },
      pullMain: async () => {
        calls.push('pullMain')
        return this.pull
      },
      install: async () => {
        calls.push('install')
        if (this.installTimeouts > 0) {
          this.installTimeouts--
          return { ok: false, note: 'timed out', timedOut: true }
        }
        return { ok: this.installOk, note: this.installOk ? '' : 'broken' }
      },
    }
  }
}

export type Fakes = {
  server: FakeServer
  ports: Ports
  spawned: { prompt: string; tier: string; description: string; agentId: string }[]
  sent: { agentId: string; text: string }[]
  appended: { agentId: string; text: string }[]
  notes: string[]
  store: { value: State | undefined }
  /** Agents that cannot be reached by `send` or `append`, and that `$.agent.list()` does not name. */
  gone: Set<string>
  /** The next spawn is refused with this text. */
  denySpawn: { reason: string | undefined }
  time: { now: number }
}

export function fakes(): Fakes {
  const server = new FakeServer()
  const f: Fakes = {
    server,
    spawned: [],
    sent: [],
    appended: [],
    notes: [],
    store: { value: undefined },
    gone: new Set(),
    denySpawn: { reason: undefined },
    time: { now: 1_000_000 },
    ports: undefined as unknown as Ports,
  }
  let n = 0
  f.ports = {
    config: TEST_CONFIG,
    now: async () => (f.time.now += 10),
    load: async () => f.store.value,
    save: async s => {
      f.store.value = JSON.parse(JSON.stringify(s))
    },
    show: () => undefined,
    forge: server.forge(),
    agentIds: async () => f.spawned.map(s => s.agentId).filter(id => !f.gone.has(id)),
    agents: {
      spawn: async a => {
        if (f.denySpawn.reason !== undefined) return { deny: f.denySpawn.reason }
        const agentId = `ag${++n}`
        f.spawned.push({ ...a, agentId })
        return { agentId }
      },
      send: async (agentId, text) => {
        if (f.gone.has(agentId)) return false
        f.sent.push({ agentId, text })
        return true
      },
      append: async (agentId, text) => {
        if (f.gone.has(agentId)) return false
        f.appended.push({ agentId, text })
        return true
      },
    },
    notify: text => {
      f.notes.push(text)
    },
    sleep: async () => undefined,
    log: () => undefined,
  }
  return f
}

// --- the machine, driven by hand ------------------------------------------------

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never

/** A machine run: feed events, read the last state and the actions of every step. */
export class Run {
  state: State = emptyState()
  now = 1_000
  last: Result = { state: this.state, actions: [] }
  all: Action[] = []

  feed(ev: DistributiveOmit<Event, 'now'>): Result {
    this.now += 1_000
    this.last = step(this.state, { ...ev, now: this.now } as Event, TEST_CONFIG)
    this.state = this.last.state
    this.all.push(...this.last.actions)
    return this.last
  }

  /** A new session starts and the liaison resumes the pipeline at once: what `restore` did before the pause. */
  restart(agents: string[]): Result {
    this.feed({ type: 'restore', agents, orphans: [], fresh: true })
    return this.feed({ type: 'resume' })
  }

  task(slug: string): Task {
    const t = this.state.tasks.find(x => x.slug === slug)
    if (!t) throw new Error(`no task ${slug}`)
    return t
  }

  actions<T extends Action['type']>(type: T): Extract<Action, { type: T }>[] {
    return this.last.actions.filter((a): a is Extract<Action, { type: T }> => a.type === type)
  }

  /** A task started, its developer spawned. */
  begin(slug: string, developer: 'developer-1' | 'developer-2' | 'developer-3' = 'developer-1', agentId = `${slug}-d1`): void {
    this.feed({ type: 'start', slug, developer, brief: `Brief of ${slug}.`, after: [] })
    this.feed({ type: 'spawned', slug, agentId, role: 'developer', tier: developer })
  }

  /** The developer ends its turn with a pull request; the check and the head are known after the poll. */
  ready(slug: string, pr: number, head: string): void {
    const t = this.task(slug)
    this.feed({ type: 'turn-end', agentId: t.agentId ?? '?', reason: 'answer', text: `Done.\nPIPELINE: pr=${pr}` })
    this.feed({ type: 'polled', slug, pr, prState: 'open', head, ci: 'pending', mergeable: true, detail: '', finished: true })
  }

  poll(slug: string, head: string, ci: 'pending' | 'success' | 'failure' | 'error', detail = '', finished = true): Result {
    return this.feed({ type: 'polled', slug, pr: this.task(slug).pr ?? 0, prState: 'open', head, ci, mergeable: true, detail, finished })
  }

  /** The reviewer was spawned and ended its turn; its review is read. */
  review(slug: string, head: string, verdict: 'APPROVED' | 'REQUEST_CHANGES' | 'none', text = '', status: 'success' | 'pending' = 'success'): Result {
    const t = this.task(slug)
    return this.feed({
      type: 'reviewed',
      slug,
      pr: t.pr ?? 0,
      head,
      verdict,
      reviewHead: verdict === 'none' ? '' : head,
      text,
      status,
      statusHead: head,
    })
  }
}

export const inputOf = (e: Input): Input => e
