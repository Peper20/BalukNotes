// The data of the pipeline: tasks, the events that reach the machine and the
// actions it asks for. Plain JSON only: the tasks are kept in `$.store`.

export const DEVELOPERS = ['developer-1', 'developer-2', 'developer-3'] as const
export type Developer = (typeof DEVELOPERS)[number]

/** A developer returns a pull request this many times before the next return goes further. */
export const MAX_ROUNDS = 2
/** The delay before a turn that ended with an API error (usage limit) is resumed. */
export const RETRY_MS = 15 * 60 * 1000
/** The install after a merge that timed out is tried again this often, at most `MAX_INSTALL_RETRIES` times. */
export const INSTALL_RETRY_MS = 15 * 60 * 1000
export const MAX_INSTALL_RETRIES = 3
/** The poll of the commit status of the pull requests in `checking`. */
export const POLL_MS = 60 * 1000
/** A check that did not finish is re-run this many times for one head; the next time the liaison is told. */
export const MAX_RERUNS = 2
/** A status that still shows the same unfinished failure this soon after a re-run is the old one: it costs no try. */
export const RERUN_GRACE_MS = 3 * 60 * 1000
const DAY_MS = 24 * 60 * 60 * 1000
/** A merged task stays in the store (and in `pipeline_status { all: true }`) this long after its merge. */
export const KEEP_MERGED_MS = 14 * DAY_MS
/** A blocked task not touched for this long is shown as stale. */
export const STALE_MS = 14 * DAY_MS
/** The pull request of a blocked task is looked at this often, to find it closed. */
export const CLOSED_POLL_MS = 60 * 60 * 1000
/** The slugs of removed merged tasks that are kept for `after`. */
export const KEEP_SLUGS = 200
/** `pipeline_status` shows this many merged tasks without `all`. */
export const SHOW_MERGED = 3

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

export type DeferredTurn = { agentId: string; reason: 'answer' | 'aborted' | 'refusal' | 'error'; text: string }

/** A turn end: `waiting` when the agent ended the turn to wait for its own background work (it wakes by itself). */
export type TurnEnd = DeferredTurn & { waiting?: boolean }

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
  /** Re-runs of the check of `head` that did not finish; cleared when the head changes. */
  reruns?: number
  /** When the last re-run was asked for (epoch ms). */
  rerunAt?: number
  /** The task is held while the pipeline runs for others: `pipeline_resume { slug }` released the rest. */
  held?: boolean
  /** Agents the session start found dead (the store said running, the engine does not know them). */
  lost?: string[]
  /** Messages of the liaison sent while the task was held: delivered by `pipeline_resume`. */
  heldSend?: string
  /** Turn ends of agents that came while the task was held: handled by `pipeline_resume`. */
  deferred?: DeferredTurn[]
  /** The texts of `pipeline_send` that reached the developer after the start, in order: the reviewer reads them as part of the task. */
  addenda?: string[]
  /** A message of the liaison appended to a running developer: sent again if the turn ends before it is read. */
  pendingSend?: string
  /** When the pull request was merged (epoch ms); `lastAt` for a task stored without it. */
  mergedAt?: number
  /** When the pull request of a blocked task was last looked at for being closed. */
  lastClosedPoll?: number
  /** The open findings: they go to a developer that takes the task over. */
  findings?: string
  blockedReason?: string
  /** A blocked task whose cleanup waits for the running agents to stop. */
  cleanupPending?: boolean
  lastEvent: string
  lastAt: number
}

/** The install after a merge timed out (the lock of heavy work was held): it is tried again from `tick`. */
export type InstallRetry = {
  /** The task whose merge the install belongs to. */
  slug: string
  /** Tries made so far. */
  tries: number
  /** When the next try is due (epoch ms). */
  at: number
  /** A try is running; not kept over a restart of the module. */
  running?: boolean
}

export type State = {
  tasks: Task[]
  installRetry?: InstallRetry
  /** Every task is held: the session started and the liaison has not resumed the pipeline. */
  paused?: boolean
  /** The slugs of merged tasks removed from `tasks` (the last `KEEP_SLUGS`): they still count as merged in `after`. */
  mergedSlugs?: string[]
}

export const isLive = (t: Task): boolean => t.stage !== 'merged' && t.stage !== 'blocked'
/** Something resuming would do for the task: a live task, an unfinished cleanup, a message kept while held. */
export const needsResume = (t: Task): boolean =>
  isLive(t) ||
  t.heldSend !== undefined ||
  (t.stage === 'blocked' && t.cleanupPending === true) ||
  (t.stage === 'merged' && t.lastEvent === 'merged; cleaning up' && t.pr !== undefined && t.head !== undefined)

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
  | ({ type: 'turn-end' } & TurnEnd)
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
      /** The check ran to its end: `success`, or a failure it commented. False for a cancelled or killed job. */
      finished: boolean
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
      /** `timeout`: the command was cut by its time limit and is tried again later. */
      install: 'ok' | 'failed' | 'skipped' | 'timeout'
      note: string
      leftovers: string[]
      conflicted: string[]
    }
  | { type: 'rerun-failed'; slug: string; head: string; reason: string }
  /** A retry of the install that timed out ended. */
  | { type: 'install-retried'; slug: string; n: number; ok: boolean; timedOut: boolean; note: string }
  /** The dead developer's worktree is dealt with: `patch` holds its unsaved changes, `note` what went wrong. */
  | { type: 'salvaged'; slug: string; agentId: string; text: string; patch?: string; note?: string }
  | { type: 'closed-polled'; slug: string; pr: number; prState: 'open' | 'closed' | 'merged' }
  | { type: 'handover-failed'; slug: string; reason: string }
  | { type: 'cleaned'; slug: string; leftovers: string[] }
  | { type: 'tick' }
  /**
   * The module started. `agents` are the ids `$.agent.list()` knows, `orphans` the worktrees no task
   * references. `fresh`: a new session (the pipeline pauses); false: a reload of the module in the same one.
   */
  | { type: 'restore'; agents: string[]; orphans: string[]; fresh: boolean }
  /** The liaison continues the held tasks: one, or all (and the pause ends). */
  | { type: 'resume'; slug?: string }
>

export type Action =
  | { type: 'spawn'; slug: string; tier: string; prompt: string; description: string; role: 'developer' | 'reviewer' }
  | { type: 'tell'; slug: string; agentId: string; text: string; how: 'send' | 'append' }
  | { type: 'poll'; slug: string; pr: number }
  /** Re-run the full check of this head (it did not finish). */
  | { type: 'rerun'; slug: string; pr: number; head: string }
  /** Save the unsaved work of a dead developer and free its worktree; `text` goes to the replacement. */
  | { type: 'salvage'; slug: string; agentId: string; text: string }
  /** Is the pull request of this blocked task closed? */
  | { type: 'poll-closed'; slug: string; pr: number }
  | { type: 'read-review'; slug: string; pr: number }
  /** Merge the pull request at this head and do the steps after a merge, under one lock. */
  | { type: 'merge'; slug: string; pr: number; head: string }
  /** The steps after a merge made elsewhere (a pull request found merged). */
  | { type: 'post-merge'; slug: string; pr: number; head: string }
  | { type: 'handover'; slug: string; previous?: string; prompt: string }
  | { type: 'cleanup'; slug: string; mode: 'blocked' | 'reviewer'; agents: string[]; pr?: number }
  /** Run the `afterMerge` command again: try `n` of `MAX_INSTALL_RETRIES`. */
  | { type: 'install-retry'; slug: string; n: number }
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
