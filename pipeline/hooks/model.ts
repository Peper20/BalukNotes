// The data of the pipeline: tasks, the events that reach the machine and the
// actions it asks for. Plain JSON only: the tasks are kept in `$.store`.

export const DEVELOPERS = ['developer-1', 'developer-2', 'developer-3'] as const
export type Developer = (typeof DEVELOPERS)[number]

/** A developer returns a pull request this many times before the next return goes further. */
export const MAX_ROUNDS = 2
/** The delay before a turn that ended with an API error (usage limit) is resumed. */
export const RETRY_MS = 15 * 60 * 1000
/** The poll of the commit status of the pull requests in `checking`. */
export const POLL_MS = 60 * 1000
/** How many merged tasks stay in the store (their slugs serve `after`). */
export const KEEP_MERGED = 100

export type Stage =
  | 'waiting' // `after` is not merged yet
  | 'developing' // a developer works, or its turn is being awaited
  | 'liaison' // waits for the liaison (`pipeline_send`)
  | 'checking' // the pull request waits for the commit status
  | 'reviewing' // a reviewer works
  | 'merging' // approved at a head with a passed status: the merge runs
  | 'merged'
  | 'blocked'

export type Ci = 'none' | 'pending' | 'success' | 'failure' | 'error'

export type AgentRef = {
  id: string
  role: 'developer' | 'reviewer'
  /** The agent type: `developer-2`, `reviewer`. */
  tier: string
  /** A turn of it is going (or a message that starts one was sent). */
  running: boolean
}

export type Task = {
  slug: string
  brief: string
  after: string[]
  stage: Stage
  /** The tier the task started on. */
  starter: Developer
  /** The tier of the current developer. */
  developer: Developer
  /** The current developer; every agent of the task is in `agents`. */
  agentId?: string
  reviewerId?: string
  agents: AgentRef[]
  pr?: number
  /** The head commit the pipeline follows. */
  head?: string
  ci: Ci
  rounds: Record<Developer, number>
  /** The developer was asked once to finish with the `PIPELINE:` line. */
  nudged: boolean
  /** An API error ended the last turn: resume the agent at this time (epoch ms). */
  retryAt?: number
  retryAgent?: string
  /** The liaison was told about the current API error streak. */
  apiNotified?: boolean
  /** Consecutive polls that saw the pull request not mergeable. */
  unmergeable: number
  /** The head the reviewer approved while its status was not `success` yet; cleared when the head changes. */
  approvedHead?: string
  /** The head for which the developer was already sent to rebase. */
  conflictHead?: string
  /** The open findings: they go to a developer that takes the task over. */
  findings?: string
  blockedReason?: string
  /** A blocked task whose cleanup waits for the running agents to stop. */
  cleanupPending?: boolean
  lastEvent: string
  lastAt: number
}

export type State = { tasks: Task[] }

export type Ending =
  | { kind: 'pr'; pr: number; liaison: boolean }
  | { kind: 'question' }

export type MergeFailure = 'conflict' | 'moved' | 'other'

type Timed<T> = T & { now: number }

export type Event = Timed<
  | { type: 'start'; slug: string; developer: Developer; brief: string; after: string[] }
  | { type: 'send'; slug: string; text: string }
  | { type: 'stop'; slug?: string; reason: string }
  | { type: 'spawned'; slug: string; agentId: string; role: 'developer' | 'reviewer'; tier: string }
  | { type: 'spawn-failed'; slug: string; role: 'developer' | 'reviewer'; reason: string }
  | { type: 'undelivered'; slug: string; agentId: string; text: string }
  | { type: 'turn-end'; agentId: string; reason: 'answer' | 'aborted' | 'refusal' | 'error'; text: string }
  | {
      type: 'polled'
      slug: string
      pr: number
      prState: 'open' | 'closed' | 'merged'
      head: string
      ci: Ci
      mergeable: boolean
      /** What failed, for a `failure`/`error` status. */
      detail: string
    }
  | {
      type: 'reviewed'
      slug: string
      pr: number
      /** The head of the pull request when the review was read. */
      head: string
      verdict: 'APPROVED' | 'REQUEST_CHANGES' | 'none'
      /** The commit the review is for. */
      reviewHead: string
      text: string
      /** The combined status of `statusHead`. */
      status: Ci
      statusHead: string
    }
  | { type: 'merge-result'; slug: string; ok: true; head: string }
  | { type: 'merge-result'; slug: string; ok: false; kind: MergeFailure; message: string }
  | {
      type: 'post-merge'
      slug: string
      pull: 'ok' | 'failed' | 'skipped'
      install: 'ok' | 'failed' | 'skipped'
      note: string
      leftovers: string[]
      conflicted: string[]
    }
  | { type: 'handover-failed'; slug: string; reason: string }
  | { type: 'cleaned'; slug: string; leftovers: string[] }
  | { type: 'tick' }
  | { type: 'restore' }
>

export type Action =
  | { type: 'spawn'; slug: string; tier: string; prompt: string; description: string; role: 'developer' | 'reviewer' }
  | { type: 'tell'; slug: string; agentId: string; text: string; how: 'send' | 'append' }
  | { type: 'poll'; slug: string; pr: number }
  | { type: 'read-review'; slug: string; pr: number }
  /** Merge the pull request at this head and do the steps after a merge, under one lock. */
  | { type: 'merge'; slug: string; pr: number; head: string }
  /** The steps after a merge made elsewhere (a pull request found merged). */
  | { type: 'post-merge'; slug: string; pr: number; head: string }
  | { type: 'handover'; slug: string; previous?: string; prompt: string }
  | { type: 'cleanup'; slug: string; mode: 'blocked' | 'reviewer'; agents: string[]; pr?: number }
  | { type: 'comment'; pr: number; text: string }
  | { type: 'notify'; text: string }

export type Result = {
  state: State
  actions: Action[]
  /** The text for the caller of a tool (`start`, `send`, `stop`). */
  reply?: string
  /** The caller's request is refused. */
  error?: string
}
