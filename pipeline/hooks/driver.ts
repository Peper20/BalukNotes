// The driver: carries out the actions of the machine through ports (the
// server, the agents, the clock, the store) and feeds the results back as events.
// register.ts builds the ports out of `$`; tests build them out of fakes.
import { type Config, needsAfterMerge } from './config'
import type { Forge } from './forge'
import { type Action, type Event, type Result, type State } from './model'
import { emptyState, step } from './machine'
import { orphanWorktrees } from './prompts'

export type Ports = {
  /** The project layer, read at session start. */
  config: Config
  now(): Promise<number>
  load(): Promise<State | undefined>
  save(state: State): Promise<void>
  /** The state changed: redraw the status line and the pane. */
  show(state: State): void
  forge: Forge
  /** The ids of the agents the engine knows now (`$.agent.list()`). */
  agentIds(): Promise<string[]>
  agents: {
    spawn(a: { prompt: string; tier: string; description: string }): Promise<{ agentId?: string; deny?: string }>
    /** To an agent that ended its turn (or is gone): resumes it. */
    send(agentId: string, text: string): Promise<boolean>
    /** To a running agent. */
    append(agentId: string, text: string): Promise<boolean>
  }
  /** A message to the liaison: it starts a turn of the liaison once idle. Not awaited. */
  notify(text: string): void
  sleep(ms: number): Promise<void>
  log(text: string): void
}

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never
export type Input = DistributiveOmit<Event, 'now'>

export type Reply = { reply?: string; error?: string }

export type Pipeline = {
  /** Loads the store and wakes what was going. */
  start(): Promise<void>
  dispatch(ev: Input): Promise<Reply>
  tick(): Promise<void>
  state(): State
  /** Resolves when no action is running and no event is waiting (for tests). */
  settle(): Promise<void>
}

/** Waits that the server needs after a merge to recompute the other pull requests. */
const SCAN_WAIT_MS = 5000

export function createPipeline(ports: Ports): Pipeline {
  let state: State = emptyState()
  let chain: Promise<unknown> = Promise.resolve()
  let mergeLock: Promise<unknown> = Promise.resolve()
  const inflight = new Set<Promise<unknown>>()
  const polling = new Set<string>()

  function dispatch(ev: Input): Promise<Reply> {
    const p = chain.then(() => handle(ev))
    chain = p.catch(() => undefined)
    return p
  }

  async function handle(input: Input): Promise<Reply> {
    const ev = { ...input, now: await ports.now() } as Event
    const result: Result = step(state, ev, ports.config)
    state = result.state
    await ports.save(state)
    ports.show(state)
    for (const action of result.actions) launch(action)
    return { reply: result.reply, error: result.error }
  }

  function launch(action: Action): void {
    const emit = (e: Input): void => void dispatch(e)
    const p = run(action, emit)
      .catch(err => ports.log(`${action.type} failed: ${String(err)}`))
      .finally(() => inflight.delete(p))
    inflight.add(p)
  }

  function taskOf(slug: string) {
    return state.tasks.find(t => t.slug === slug)
  }

  async function run(a: Action, emit: (e: Input) => void): Promise<void> {
    switch (a.type) {
      case 'notify':
        ports.notify(a.text)
        return
      case 'comment':
        await ports.forge.comment(a.pr, a.text).catch(err => ports.log(`comment on ${a.pr}: ${String(err)}`))
        return
      case 'spawn':
        await spawn(a.slug, a.role, a.tier, a.prompt, a.description, emit)
        return
      case 'tell': {
        const delivered =
          a.how === 'append'
            ? (await ports.agents.append(a.agentId, a.text)) || (await ports.agents.send(a.agentId, a.text))
            : (await ports.agents.send(a.agentId, a.text)) || (await ports.agents.append(a.agentId, a.text))
        if (!delivered) emit({ type: 'undelivered', slug: a.slug, agentId: a.agentId, text: a.text })
        return
      }
      case 'poll': {
        const key = `${a.slug}#${a.pr}`
        if (polling.has(key)) return
        polling.add(key)
        try {
          const pr = await ports.forge.pr(a.pr)
          const st = pr.state === 'open' ? await ports.forge.status(pr.head, a.pr) : { state: 'none' as const, detail: '', finished: true }
          emit({
            type: 'polled',
            slug: a.slug,
            pr: a.pr,
            prState: pr.state,
            head: pr.head,
            ci: st.state,
            mergeable: pr.mergeable,
            detail: st.detail,
            finished: st.finished,
          })
        } finally {
          polling.delete(key)
        }
        return
      }
      case 'rerun':
        await ports.forge.rerun(a.head).catch(err => emit({ type: 'rerun-failed', slug: a.slug, head: a.head, reason: String(err) }))
        return
      case 'salvage': {
        const out = await ports.forge.salvage({ slug: a.slug, agentId: a.agentId }).catch(err => ({ note: String(err) }))
        emit({ type: 'salvaged', slug: a.slug, agentId: a.agentId, text: a.text, ...out })
        return
      }
      case 'poll-closed': {
        const pr = await ports.forge.pr(a.pr).catch(err => {
          ports.log(`pull request ${a.pr} of ${a.slug}: ${String(err)}`)
          return undefined
        })
        if (pr) emit({ type: 'closed-polled', slug: a.slug, pr: a.pr, prState: pr.state })
        return
      }
      case 'read-review': {
        try {
          const pr = await ports.forge.pr(a.pr)
          const [review, status] = await Promise.all([ports.forge.review(a.pr, pr.head), ports.forge.status(pr.head, a.pr)])
          emit({
            type: 'reviewed',
            slug: a.slug,
            pr: a.pr,
            head: pr.head,
            verdict: review.verdict,
            reviewHead: review.commit,
            text: review.text,
            status: status.state,
            statusHead: pr.head,
          })
        } catch (err) {
          const t = taskOf(a.slug)
          ports.notify(`[pipeline] ${a.slug}${t?.pr === undefined ? '' : ` (pull request #${t.pr})`}: could not read the review: ${String(err)}`)
        }
        return
      }
      case 'merge':
        await locked(() => merge(a.slug, a.pr, a.head, emit))
        return
      case 'post-merge':
        await locked(async () => emit(await postMerge(a.slug, a.pr, a.head)))
        return
      case 'handover': {
        const left = a.previous === undefined ? [] : await ports.forge.removeAgents([a.previous])
        if (left.length > 0) {
          emit({ type: 'handover-failed', slug: a.slug, reason: left.join('; ') })
          return
        }
        await spawn(a.slug, 'developer', 'developer-3', a.prompt, a.slug, emit)
        return
      }
      case 'cleanup': {
        const left = await ports.forge.removeAgents(a.agents)
        if (a.mode === 'blocked') left.push(...(await ports.forge.cleanupBlocked({ slug: a.slug, pr: a.pr })))
        emit({ type: 'cleaned', slug: a.slug, leftovers: left })
        return
      }
    }
  }

  async function spawn(
    slug: string,
    role: 'developer' | 'reviewer',
    tier: string,
    prompt: string,
    description: string,
    emit: (e: Input) => void,
  ): Promise<void> {
    try {
      const r = await ports.agents.spawn({ prompt, tier, description })
      if (r.deny !== undefined || r.agentId === undefined) {
        emit({ type: 'spawn-failed', slug, role, reason: r.deny ?? 'no agent id' })
      } else {
        emit({ type: 'spawned', slug, agentId: r.agentId, role, tier })
      }
    } catch (err) {
      emit({ type: 'spawn-failed', slug, role, reason: String(err) })
    }
  }

  /** One merge at a time, with the steps after it: the next merge sees the updated main. */
  function locked(job: () => Promise<void>): Promise<void> {
    const p = mergeLock.then(job)
    mergeLock = p.catch(() => undefined)
    return p
  }

  async function merge(slug: string, pr: number, head: string, emit: (e: Input) => void): Promise<void> {
    let outcome: Awaited<ReturnType<Forge['merge']>>
    try {
      outcome = await ports.forge.merge(pr, head, `task/${slug}`)
    } catch (err) {
      // The server could not be read: the liaison decides, there is no automatic retry.
      outcome = { ok: false, kind: 'other', message: err instanceof Error ? err.message : String(err) }
    }
    if (!outcome.ok) {
      emit({ type: 'merge-result', slug, ok: false, kind: outcome.kind, message: outcome.message })
      return
    }
    emit({ type: 'merge-result', slug, ok: true, head })
    emit(await postMerge(slug, pr, head))
  }

  async function postMerge(slug: string, pr: number, head: string): Promise<Input> {
    const task = taskOf(slug)
    const leftovers = [
      ...(await ports.forge.removeAgents(task?.agents.map(a => a.id) ?? [])),
      ...(await ports.forge.cleanupMerged({ slug, pr, head })),
    ]
    const pulled = await ports.forge.pullMain()
    let install: 'ok' | 'failed' | 'skipped' = 'skipped'
    let note = pulled.note
    if (pulled.pull === 'ok' && needsAfterMerge(pulled.changed, ports.config.afterMerge)) {
      const r = await ports.forge.install()
      install = r.ok ? 'ok' : 'failed'
      note = r.note
    }
    const others = state.tasks.filter(
      t => t.slug !== slug && t.pr !== undefined && ['checking', 'reviewing', 'developing'].includes(t.stage),
    )
    const conflicted: string[] = []
    if (others.length > 0) await ports.sleep(SCAN_WAIT_MS)
    for (const o of others) {
      try {
        if (o.pr === undefined) continue
        const first = await ports.forge.pr(o.pr)
        if (first.state !== 'open' || first.mergeable) continue
        // "Not mergeable" is also what the server says while it still checks: ask twice.
        await ports.sleep(SCAN_WAIT_MS)
        if (!(await ports.forge.pr(o.pr)).mergeable) conflicted.push(o.slug)
      } catch (err) {
        ports.log(`conflict scan of ${o.slug}: ${String(err)}`)
      }
    }
    return { type: 'post-merge', slug, pull: pulled.pull, install, note, leftovers, conflicted }
  }

  /** Worktrees of the pipeline that no task references: told to the liaison, not removed. */
  async function reportOrphans(): Promise<void> {
    try {
      const found = await ports.forge.unreferenced({
        agents: state.tasks.flatMap(t => t.agents.map(a => a.id)),
        prs: state.tasks.flatMap(t => (t.pr === undefined ? [] : [t.pr])),
      })
      if (found.length > 0) ports.notify(orphanWorktrees(found))
    } catch (err) {
      ports.log(`worktree listing: ${String(err)}`)
    }
  }

  async function settle(): Promise<void> {
    for (;;) {
      await chain
      if (inflight.size === 0) {
        await chain
        if (inflight.size === 0) return
      }
      await Promise.all([...inflight])
    }
  }

  return {
    async start() {
      const stored = await ports.load()
      if (stored) state = stored
      ports.show(state)
      await dispatch({ type: 'restore', agents: await ports.agentIds() })
      await reportOrphans()
    },
    dispatch,
    tick: async () => {
      await dispatch({ type: 'tick' })
    },
    state: () => state,
    settle,
  }
}
