// The state machine of the pipeline: (state, event, config) -> (new state, actions).
// No clock, no server, no agents: everything the pipeline decides is here, the
// driver (driver.ts) only carries the actions out and feeds their results back
// as events. The input state is never changed.
import { type Config, pullsUrl } from './config'
import {
  type AgentRef,
  type Developer,
  DEVELOPERS,
  type Event,
  CLOSED_POLL_MS,
  KEEP_MERGED_MS,
  KEEP_SLUGS,
  MAX_RERUNS,
  MAX_ROUNDS,
  RETRY_MS,
  type Result,
  type State,
  type Task,
} from './model'
import * as P from './prompts'

export const emptyState = (): State => ({ tasks: [] })

const short = (sha?: string): string => (sha ?? '').slice(0, 7)
const find = (s: State, slug: string): Task | undefined => s.tasks.find(t => t.slug === slug)
const owner = (s: State, agentId: string): Task | undefined => s.tasks.find(t => t.agents.some(a => a.id === agentId))
const agentOf = (t: Task, id: string): AgentRef | undefined => t.agents.find(a => a.id === id)
const isLive = (t: Task): boolean => t.stage !== 'merged' && t.stage !== 'blocked'
const note = (t: Task, now: number, text: string): void => {
  t.lastEvent = text
  t.lastAt = now
}
const roundsText = (t: Task): string => DEVELOPERS.map(d => `${d} ${t.rounds[d]}`).join(', ')

type Event_<T extends Event['type']> = Extract<Event, { type: T }>

export function step(prev: State, ev: Event, cfg: Config): Result {
  const state: State = JSON.parse(JSON.stringify(prev))
  const out: Result = { state, actions: [] }
  switch (ev.type) {
    case 'start':
      onStart(state, ev, out, cfg)
      break
    case 'send':
      onSend(state, ev, out, cfg)
      break
    case 'stop':
      onStop(state, ev, out)
      break
    case 'spawned':
      onSpawned(state, ev, out)
      break
    case 'spawn-failed':
      onSpawnFailed(state, ev, out)
      break
    case 'undelivered':
      onUndelivered(state, ev, out, cfg)
      break
    case 'turn-end':
      onTurnEnd(state, ev, out)
      break
    case 'polled':
      onPolled(state, ev, out, cfg)
      break
    case 'reviewed':
      onReviewed(state, ev, out, cfg)
      break
    case 'merge-result':
      onMergeResult(state, ev, out, cfg)
      break
    case 'post-merge':
      onPostMerge(state, ev, out, cfg)
      break
    case 'rerun-failed':
      onRerunFailed(state, ev)
      break
    case 'salvaged':
      onSalvaged(state, ev, out, cfg)
      break
    case 'closed-polled':
      onClosedPolled(state, ev, out)
      break
    case 'handover-failed':
      onHandoverFailed(state, ev, out)
      break
    case 'cleaned':
      onCleaned(state, ev, out)
      break
    case 'tick':
      onTick(state, ev, out, cfg)
      break
    case 'restore':
      onRestore(state, ev, out, cfg)
      break
  }
  return out
}

// --- helpers that add actions ------------------------------------------------

function notify(t: Task, why: string, out: Result): void {
  out.actions.push({ type: 'notify', text: P.liaisonEvent(t, why) })
}

/** A message to the current developer; a running one is appended to, an ended one resumed. */
function tell(t: Task, agentId: string, text: string, out: Result, how?: 'send' | 'append'): void {
  const a = agentOf(t, agentId)
  const resolved = how ?? (a?.running ? 'append' : 'send')
  out.actions.push({ type: 'tell', slug: t.slug, agentId, text, how: resolved })
  if (resolved === 'send') t.pendingSend = undefined
  if (a) a.running = true
}

/** `text` with the message of the liaison that may have been lost; the message is taken off the task. */
function withPending(t: Task, text: string): string {
  const pending = t.pendingSend
  t.pendingSend = undefined
  if (pending === undefined || text.includes(pending)) return text
  return `${text.trim()}\n\n${P.lostMessage(pending)}`
}

/** The developer is gone: its unsaved work is saved, then a new one of the same tier continues (`onSalvaged`). */
function replaceDeveloper(t: Task, agentId: string, text: string, out: Result): void {
  t.agentId = undefined
  out.actions.push({ type: 'salvage', slug: t.slug, agentId, text: withPending(t, text) })
}

function spawnDeveloper(t: Task, prompt: string, out: Result): void {
  t.agentId = undefined
  out.actions.push({ type: 'spawn', slug: t.slug, role: 'developer', tier: t.developer, prompt, description: t.slug })
}

/** To the developer of the task: a message, or a new developer of the same tier when none is left. */
function toDeveloper(t: Task, text: string, out: Result, cfg: Config): void {
  if (t.agentId !== undefined) tell(t, t.agentId, text, out)
  else spawnDeveloper(t, P.continuePrompt(t, text, cfg), out)
}

function spawnReviewer(t: Task, out: Result, cfg: Config): void {
  if (t.pr === undefined || t.head === undefined) return
  t.reviewerId = undefined
  out.actions.push({
    type: 'spawn',
    slug: t.slug,
    role: 'reviewer',
    tier: 'reviewer',
    prompt: P.reviewerPrompt(t, t.pr, t.head, cfg),
    description: `review ${t.slug}`,
  })
}

function begin(t: Task, now: number, out: Result, cfg: Config): void {
  t.stage = 'developing'
  note(t, now, `started on ${t.developer}`)
  spawnDeveloper(t, P.developerPrompt(t, cfg), out)
}

const isMerged = (s: State, slug: string): boolean => find(s, slug)?.stage === 'merged' || (s.mergedSlugs ?? []).includes(slug)
const ready = (s: State, t: Task): boolean => t.after.every(a => isMerged(s, a))

function startWaiting(s: State, now: number, out: Result, cfg: Config): string[] {
  const started: string[] = []
  for (const t of s.tasks) {
    if (t.stage === 'waiting' && ready(s, t)) {
      begin(t, now, out, cfg)
      started.push(t.slug)
    }
  }
  return started
}

function runningAgents(t: Task): AgentRef[] {
  return t.agents.filter(a => a.running)
}

function maybeCleanup(t: Task, out: Result): void {
  if (t.stage === 'blocked' && t.cleanupPending && runningAgents(t).length === 0) {
    t.cleanupPending = false
    out.actions.push({ type: 'cleanup', slug: t.slug, mode: 'blocked', agents: t.agents.map(a => a.id), pr: t.pr })
  }
}

function block(t: Task, now: number, reason: string, findings: string, out: Result): void {
  t.stage = 'blocked'
  t.blockedReason = reason
  t.findings = findings
  t.cleanupPending = true
  note(t, now, `blocked: ${reason}`)
  if (t.pr !== undefined) out.actions.push({ type: 'comment', pr: t.pr, text: P.blockedComment(t, reason, findings) })
  maybeCleanup(t, out)
  notify(t, `blocked: ${reason}. The pull request and the pushed branch stay.`, out)
}

/** The pull request goes back to a developer with findings: a round, a hand-over or a block. */
function giveBack(t: Task, now: number, kind: 'check' | 'review', findings: string, out: Result, cfg: Config): void {
  const cur = t.developer
  const used = t.rounds[cur]
  t.findings = findings
  t.nudged = false
  if (used < MAX_ROUNDS) {
    t.rounds[cur] = used + 1
    t.stage = 'developing'
    const text =
      kind === 'check' ? P.checkFailedMessage(t, used + 1, findings) : P.changesMessage(t, used + 1, findings)
    note(t, now, `${kind === 'check' ? 'check failed' : 'changes requested'}: round ${used + 1} of ${cur}`)
    toDeveloper(t, text, out, cfg)
    return
  }
  if (cur === 'developer-3') {
    block(t, now, 'developer-3 used its two rounds without an approved pull request', findings, out)
    return
  }
  const previous = t.agentId
  t.developer = 'developer-3'
  t.rounds['developer-3'] = 1
  t.stage = 'developing'
  t.agentId = undefined
  note(t, now, `${cur} used its two rounds: handed over to developer-3`)
  out.actions.push({
    type: 'handover',
    slug: t.slug,
    previous,
    prompt: P.continuePrompt(
      t,
      `The previous developer (${cur}) used its two rounds without an approved pull request. You take the task over: this is your first round. The open findings:\n\n${findings.trim()}`,
      cfg,
    ),
  })
}

/** The pull request conflicts with main: the developer rebases. Not a round. */
function sendToRebase(t: Task, now: number, out: Result, cfg: Config): void {
  t.conflictHead = t.head ?? ''
  t.unmergeable = 0
  t.stage = 'developing'
  t.nudged = false
  note(t, now, 'conflicts with main: the developer rebases')
  toDeveloper(t, P.rebaseMessage(t), out, cfg)
}

// --- events -----------------------------------------------------------------

function onStart(s: State, ev: Event_<'start'>, out: Result, cfg: Config): void {
  if (!/^[a-z0-9][a-z0-9._-]*$/.test(ev.slug)) {
    out.error = `bad slug "${ev.slug}": lower case letters, digits, "-", "_" and "."`
    return
  }
  if (find(s, ev.slug)) {
    out.error = `the task "${ev.slug}" exists (${find(s, ev.slug)?.stage})`
    return
  }
  if (!DEVELOPERS.includes(ev.developer)) {
    out.error = `the developer must be one of ${DEVELOPERS.join(', ')}`
    return
  }
  if ((s.mergedSlugs ?? []).includes(ev.slug)) {
    out.error = `the task "${ev.slug}" exists (merged)`
    return
  }
  const unknown = ev.after.filter(a => !find(s, a) && !isMerged(s, a))
  if (unknown.length > 0) {
    out.error = `"after" names unknown tasks: ${unknown.join(', ')} (start them first)`
    return
  }
  const t: Task = {
    slug: ev.slug,
    brief: ev.brief,
    after: ev.after,
    stage: 'waiting',
    starter: ev.developer,
    developer: ev.developer,
    agents: [],
    ci: 'none',
    rounds: { 'developer-1': 0, 'developer-2': 0, 'developer-3': 0 },
    nudged: false,
    unmergeable: 0,
    lastEvent: 'created',
    lastAt: ev.now,
  }
  s.tasks.push(t)
  if (ready(s, t)) {
    begin(t, ev.now, out, cfg)
    out.reply = `${t.slug}: ${t.developer} is starting on ${P.branchOf(t.slug)}`
  } else {
    const missing = t.after.filter(a => !isMerged(s, a))
    note(t, ev.now, `waits for ${missing.join(', ')}`)
    out.reply = `${t.slug}: waits until ${missing.join(', ')} ${missing.length === 1 ? 'is' : 'are'} merged`
  }
}

function onSend(s: State, ev: Event_<'send'>, out: Result, cfg: Config): void {
  const t = find(s, ev.slug)
  if (!t) {
    out.error = `no task "${ev.slug}"`
    return
  }
  switch (t.stage) {
    case 'merged':
      out.error = `${t.slug} is merged`
      return
    case 'waiting':
      t.brief = `${t.brief.trim()}\n\nAddendum from the liaison:\n${ev.text.trim()}`
      note(t, ev.now, 'the liaison added to the brief')
      out.reply = `${t.slug}: added to the brief; the task has not started`
      return
    case 'blocked':
      // The liaison resumes a blocked task: a new developer-3 on the branch and the pull request, rounds counted afresh.
      t.developer = 'developer-3'
      t.rounds = { 'developer-1': 0, 'developer-2': 0, 'developer-3': 0 }
      t.stage = 'developing'
      t.blockedReason = undefined
      t.pendingSend = undefined
      t.cleanupPending = false
      t.nudged = false
      t.retryAt = undefined
      note(t, ev.now, 'resumed by the liaison on developer-3')
      spawnDeveloper(t, P.continuePrompt(t, `The task was blocked and is resumed.\n\n${ev.text.trim()}\n\nOpen findings:\n${t.findings ?? '(none recorded)'}`, cfg), out)
      out.reply = `${t.slug}: resumed on a new developer-3`
      return
    case 'developing':
      if (t.agentId === undefined) {
        out.error = `${t.slug}: the developer is starting, send again in a moment`
        return
      }
      // A turn that is ending may not read it: kept to be sent again (onTurnEnd).
      const midTurn = agentOf(t, t.agentId)?.running === true
      tell(t, t.agentId, ev.text, out)
      if (midTurn) t.pendingSend = [t.pendingSend, ev.text.trim()].filter(x => x !== undefined && x !== '').join('\n\n')
      note(t, ev.now, 'the liaison sent a message')
      out.reply = `${t.slug}: sent to ${t.developer}`
      return
    default:
      // liaison, checking, reviewing, merging: the pull request goes back to the developer.
      t.stage = 'developing'
      t.nudged = false
      t.retryAt = undefined
      note(t, ev.now, 'the liaison sent a message')
      toDeveloper(t, ev.text, out, cfg)
      out.reply = `${t.slug}: sent to ${t.developer}; the task is developing`
  }
}

function onStop(s: State, ev: Event_<'stop'>, out: Result): void {
  const targets = ev.slug === undefined ? s.tasks.filter(isLive) : s.tasks.filter(t => t.slug === ev.slug)
  if (ev.slug !== undefined && targets.length === 0) {
    out.error = `no task "${ev.slug}"`
    return
  }
  const stopped: string[] = []
  for (const t of targets) {
    if (!isLive(t)) continue
    const reason = `stopped by the user: ${ev.reason}`
    for (const a of runningAgents(t)) {
      out.actions.push({ type: 'tell', slug: t.slug, agentId: a.id, text: P.stopMessage(ev.reason), how: 'append' })
    }
    t.stage = 'blocked'
    t.blockedReason = reason
    t.cleanupPending = true
    t.retryAt = undefined
    t.pendingSend = undefined
    note(t, ev.now, reason)
    if (t.pr !== undefined) out.actions.push({ type: 'comment', pr: t.pr, text: P.blockedComment(t, reason, '') })
    maybeCleanup(t, out)
    stopped.push(t.slug)
  }
  out.reply = stopped.length === 0 ? 'nothing to stop' : `stopped: ${stopped.join(', ')}`
}

function onSpawned(s: State, ev: Event_<'spawned'>, out: Result): void {
  const t = find(s, ev.slug)
  if (!t) return
  t.agents.push({ id: ev.agentId, role: ev.role, tier: ev.tier, running: true })
  if (ev.role === 'developer') {
    t.agentId = ev.agentId
    t.developer = (DEVELOPERS as readonly string[]).includes(ev.tier) ? (ev.tier as Developer) : t.developer
    note(t, ev.now, `${ev.tier} started (${ev.agentId})`)
  } else {
    t.reviewerId = ev.agentId
    note(t, ev.now, `reviewer started (${ev.agentId})`)
  }
  if (t.stage === 'blocked') {
    // The task was stopped while the agent started.
    out.actions.push({ type: 'tell', slug: t.slug, agentId: ev.agentId, text: P.stopMessage(t.blockedReason ?? 'stopped'), how: 'append' })
  }
}

function onSpawnFailed(s: State, ev: Event_<'spawn-failed'>, out: Result): void {
  const t = find(s, ev.slug)
  if (!t || !isLive(t)) return
  t.stage = 'liaison'
  note(t, ev.now, `${ev.role} could not start: ${ev.reason}`)
  notify(t, `the ${ev.role} could not be started: ${ev.reason}. Answer with pipeline_send to try again.`, out)
}

function onUndelivered(s: State, ev: Event_<'undelivered'>, out: Result, cfg: Config): void {
  const t = find(s, ev.slug)
  const a = t && agentOf(t, ev.agentId)
  if (!t || !a) return
  a.running = false
  if (t.stage === 'blocked') {
    maybeCleanup(t, out)
    return
  }
  if (a.role === 'developer' && ev.agentId === t.agentId && isLive(t)) {
    note(t, ev.now, `${a.tier} is gone: a new one continues`)
    replaceDeveloper(t, ev.agentId, ev.text, out)
  } else if (a.role === 'reviewer' && ev.agentId === t.reviewerId && t.stage === 'reviewing') {
    note(t, ev.now, 'the reviewer is gone: a new one reviews')
    spawnReviewer(t, out, cfg)
  }
}

function onTurnEnd(s: State, ev: Event_<'turn-end'>, out: Result): void {
  const t = owner(s, ev.agentId)
  const a = t && agentOf(t, ev.agentId)
  if (!t || !a) return
  a.running = false

  if (t.stage === 'blocked') {
    maybeCleanup(t, out)
    return
  }
  if (a.role === 'reviewer') {
    if (t.stage !== 'reviewing' || ev.agentId !== t.reviewerId || t.pr === undefined) {
      out.actions.push({ type: 'cleanup', slug: t.slug, mode: 'reviewer', agents: [ev.agentId] })
      return
    }
    if (ev.reason === 'error') {
      scheduleRetry(t, ev, out)
      return
    }
    t.retryAt = undefined
    out.actions.push({ type: 'read-review', slug: t.slug, pr: t.pr })
    return
  }

  // A developer. Only the current one in `developing` is heard.
  if (ev.agentId !== t.agentId || t.stage !== 'developing') {
    note(t, ev.now, `${a.tier} ended a turn (${ev.reason}) outside its stage`)
    return
  }
  if (ev.reason === 'error') {
    scheduleRetry(t, ev, out)
    return
  }
  t.retryAt = undefined
  t.apiNotified = false
  if (t.pendingSend !== undefined) {
    // The liaison wrote while this turn was ending, so the turn may not have read it: the ending does not count.
    note(t, ev.now, 'a message of the liaison came while the turn was ending: sent again')
    tell(t, ev.agentId, P.lateSendMessage(t.pendingSend), out, 'send')
    return
  }
  const ending = P.parseEnding(ev.text)
  if (!ending) {
    if (!t.nudged) {
      t.nudged = true
      note(t, ev.now, `ended (${ev.reason}) without the PIPELINE line: asked to finish`)
      tell(t, ev.agentId, P.nudgeMessage(), out, 'send')
    } else {
      t.stage = 'liaison'
      note(t, ev.now, 'ended twice without the PIPELINE line')
      notify(t, `the ${t.developer} ended twice without the PIPELINE line (${ev.reason}); see its report`, out)
    }
    return
  }
  t.nudged = false
  if (ending.kind === 'question') {
    t.stage = 'liaison'
    note(t, ev.now, 'question for the liaison')
    notify(t, 'the developer has a question and no pull request; see the developer\'s report', out)
    return
  }
  t.pr = ending.pr
  if (ending.liaison) {
    t.stage = 'liaison'
    note(t, ev.now, `PR #${ending.pr} waits for the liaison`)
    notify(t, 'the developer\'s report needs you before the review (architecture additions, deferred items or a question); see the developer\'s report', out)
    return
  }
  t.stage = 'checking'
  t.ci = 'pending'
  t.head = undefined
  t.unmergeable = 0
  note(t, ev.now, `PR #${ending.pr} is ready: waiting for the check`)
  out.actions.push({ type: 'poll', slug: t.slug, pr: ending.pr })
}

function scheduleRetry(t: Task, ev: Event_<'turn-end'>, out: Result): void {
  t.retryAt = ev.now + RETRY_MS
  t.retryAgent = ev.agentId
  note(t, ev.now, 'the turn ended with an API error: resumes in 15 minutes')
  if (!t.apiNotified) {
    t.apiNotified = true
    notify(t, 'a turn ended with an API error (a usage limit?); the pipeline resumes it every 15 minutes', out)
  }
}

function onPolled(s: State, ev: Event_<'polled'>, out: Result, cfg: Config): void {
  const t = find(s, ev.slug)
  if (!t || t.stage !== 'checking' || t.pr !== ev.pr) return
  if (ev.prState === 'merged') {
    t.head = ev.head
    mergedNow(t, ev.now, true, out)
    return
  }
  if (ev.prState === 'closed') {
    block(t, ev.now, 'the pull request was closed on the server', '', out)
    return
  }
  if (t.head !== undefined && t.head !== ev.head) note(t, ev.now, `new head ${short(ev.head)}`)
  if (t.approvedHead !== ev.head) t.approvedHead = undefined
  if (t.head !== ev.head) t.reruns = 0
  t.head = ev.head
  t.ci = ev.ci
  t.unmergeable = ev.mergeable ? 0 : t.unmergeable + 1
  // The server says "not mergeable" while it checks too: two polls in a row make a conflict.
  if (t.unmergeable >= 2 && t.conflictHead !== ev.head) {
    sendToRebase(t, ev.now, out, cfg)
    return
  }
  switch (ev.ci) {
    case 'success':
      if (t.approvedHead === ev.head && t.pr !== undefined) {
        // The reviewer approved this very head while its status was not `success` yet: no second review.
        t.stage = 'merging'
        note(t, ev.now, `check passed at ${short(ev.head)}, approved earlier: merging`)
        out.actions.push({ type: 'merge', slug: t.slug, pr: t.pr, head: ev.head })
        return
      }
      t.stage = 'reviewing'
      note(t, ev.now, `check passed at ${short(ev.head)}: review`)
      spawnReviewer(t, out, cfg)
      return
    case 'failure':
    case 'error':
      if (ev.finished) giveBack(t, ev.now, 'check', ev.detail, out, cfg)
      else rerunCheck(t, ev, out)
      return
    default:
      note(t, ev.now, `check ${ev.ci} at ${short(ev.head)}`)
  }
}

/** The check was cancelled or could not run: not a round of the developer. Re-run it, or tell the liaison. */
function rerunCheck(t: Task, ev: Event_<'polled'>, out: Result): void {
  const used = t.reruns ?? 0
  if (used >= MAX_RERUNS) {
    t.stage = 'liaison'
    note(t, ev.now, `the check of ${short(ev.head)} did not finish ${used + 1} times`)
    notify(t, `the check of ${short(ev.head)} did not finish ${used + 1} times: ${ev.detail.trim()}`, out)
    return
  }
  t.reruns = used + 1
  t.ci = 'pending'
  note(t, ev.now, `the check of ${short(ev.head)} did not finish: re-run ${used + 1} of ${MAX_RERUNS}`)
  if (t.pr !== undefined) out.actions.push({ type: 'rerun', slug: t.slug, pr: t.pr, head: ev.head })
}

function onRerunFailed(s: State, ev: Event_<'rerun-failed'>): void {
  const t = find(s, ev.slug)
  if (!t || t.stage !== 'checking') return
  note(t, ev.now, `the re-run of the check of ${short(ev.head)} failed: ${ev.reason}`)
}

function onReviewed(s: State, ev: Event_<'reviewed'>, out: Result, cfg: Config): void {
  const t = find(s, ev.slug)
  if (!t || t.stage !== 'reviewing' || t.pr !== ev.pr) return
  if (t.reviewerId !== undefined) {
    out.actions.push({ type: 'cleanup', slug: t.slug, mode: 'reviewer', agents: [t.reviewerId] })
  }
  if (ev.head !== t.head) {
    // A new head was pushed while the review ran: the review is stale, the check starts over.
    t.head = ev.head
    t.reruns = 0
    t.approvedHead = undefined
    t.stage = 'checking'
    t.ci = 'pending'
    note(t, ev.now, `new head ${short(ev.head)} during the review: checking again`)
    out.actions.push({ type: 'poll', slug: t.slug, pr: ev.pr })
    return
  }
  switch (ev.verdict) {
    case 'APPROVED':
      if (ev.reviewHead === ev.head && ev.statusHead === ev.head && ev.status === 'success') {
        t.stage = 'merging'
        note(t, ev.now, `approved at ${short(ev.head)}: merging`)
        out.actions.push({ type: 'merge', slug: t.slug, pr: ev.pr, head: ev.head })
      } else {
        t.stage = 'checking'
        t.ci = ev.status
        if (ev.reviewHead === ev.head) t.approvedHead = ev.head
        note(t, ev.now, `approved, but the status is ${ev.status} at ${short(ev.head)}: waiting`)
        out.actions.push({ type: 'poll', slug: t.slug, pr: ev.pr })
      }
      return
    case 'REQUEST_CHANGES':
      giveBack(t, ev.now, 'review', ev.text, out, cfg)
      return
    case 'none':
      t.stage = 'liaison'
      note(t, ev.now, 'the reviewer ended without a verdict')
      notify(t, 'the reviewer ended without a verdict on the server; look at the pull request', out)
  }
}

/**
 * The pull request is merged. `post` asks for the steps after a merge (cleanup,
 * pull, install, report): the merge action of the driver does them itself.
 */
function mergedNow(t: Task, now: number, post: boolean, out: Result): void {
  if (t.stage === 'merged') return
  t.stage = 'merged'
  t.retryAt = undefined
  t.mergedAt = now
  note(t, now, 'merged; cleaning up')
  if (post && t.pr !== undefined && t.head !== undefined) {
    out.actions.push({ type: 'post-merge', slug: t.slug, pr: t.pr, head: t.head })
  }
}

function onMergeResult(s: State, ev: Event_<'merge-result'>, out: Result, cfg: Config): void {
  const t = find(s, ev.slug)
  if (!t) return
  if (ev.ok) {
    t.head = ev.head
    mergedNow(t, ev.now, false, out)
    return
  }
  if (t.stage !== 'merging') return
  switch (ev.kind) {
    case 'conflict':
      sendToRebase(t, ev.now, out, cfg)
      return
    case 'moved':
      t.stage = 'checking'
      t.ci = 'pending'
      note(t, ev.now, 'the head moved during the merge: checking again')
      if (t.pr !== undefined) out.actions.push({ type: 'poll', slug: t.slug, pr: t.pr })
      return
    default:
      t.stage = 'liaison'
      note(t, ev.now, `merge failed: ${ev.message}`)
      notify(t, `the merge failed: ${ev.message}`, out)
  }
}

function onPostMerge(s: State, ev: Event_<'post-merge'>, out: Result, cfg: Config): void {
  const t = find(s, ev.slug)
  if (!t) return
  note(t, ev.now, 'merged and cleaned up')
  const told: string[] = []
  for (const slug of ev.conflicted) {
    const c = find(s, slug)
    if (!c || c === t || c.pr === undefined || (c.conflictHead !== undefined && c.conflictHead === c.head)) continue
    if (c.stage === 'checking' || c.stage === 'reviewing' || c.stage === 'developing') {
      sendToRebase(c, ev.now, out, cfg)
      told.push(slug)
    }
  }
  const started = startWaiting(s, ev.now, out, cfg)
  const lines = [
    `[pipeline] ${t.slug} merged: ${pullsUrl(cfg)}${t.pr}`,
    `rounds: ${roundsText(t)}`,
    `main: pull ${ev.pull}; install: ${ev.install}${ev.note === '' ? '' : ` (${ev.note})`}`,
    ev.leftovers.length === 0 ? 'cleanup: nothing left' : `cleanup left:\n- ${ev.leftovers.join('\n- ')}`,
  ]
  if (told.length > 0) lines.push(`conflicts: ${told.join(', ')} told to rebase`)
  if (started.length > 0) lines.push(`started: ${started.join(', ')}`)
  out.actions.push({ type: 'notify', text: lines.join('\n') })
}

function onHandoverFailed(s: State, ev: Event_<'handover-failed'>, out: Result): void {
  const t = find(s, ev.slug)
  if (!t || !isLive(t)) return
  block(t, ev.now, `the hand-over to developer-3 failed: ${ev.reason}`, t.findings ?? '', out)
}

function onCleaned(s: State, ev: Event_<'cleaned'>, out: Result): void {
  const t = find(s, ev.slug)
  if (!t || ev.leftovers.length === 0) return
  note(t, ev.now, `cleanup left: ${ev.leftovers.join('; ')}`)
  if (t.stage === 'blocked') notify(t, `cleanup left:\n- ${ev.leftovers.join('\n- ')}`, out)
}

/** Retention: a merged task goes 14 days after its merge, its slug is kept for `after`. */
function pruneMerged(s: State, now: number): void {
  const gone = s.tasks.filter(t => t.stage === 'merged' && now - (t.mergedAt ?? t.lastAt) > KEEP_MERGED_MS)
  if (gone.length === 0) return
  const drop = new Set(gone.map(t => t.slug))
  s.tasks = s.tasks.filter(t => !drop.has(t.slug))
  const kept = (s.mergedSlugs ?? []).filter(slug => !drop.has(slug))
  s.mergedSlugs = [...kept, ...gone.map(t => t.slug)].slice(-KEEP_SLUGS)
}

function onClosedPolled(s: State, ev: Event_<'closed-polled'>, out: Result): void {
  const t = find(s, ev.slug)
  if (!t || t.stage !== 'blocked' || t.pr !== ev.pr || t.cleanupPending || ev.prState !== 'closed') return
  s.tasks = s.tasks.filter(x => x !== t)
  out.actions.push({ type: 'notify', text: `[pipeline] ${t.slug} removed: its pull request #${ev.pr} was closed unmerged` })
}

function onTick(s: State, ev: Event_<'tick'>, out: Result, cfg: Config): void {
  pruneMerged(s, ev.now)
  for (const t of s.tasks) {
    if (
      t.stage === 'blocked' &&
      t.pr !== undefined &&
      !t.cleanupPending &&
      (t.lastClosedPoll === undefined || ev.now - t.lastClosedPoll >= CLOSED_POLL_MS)
    ) {
      t.lastClosedPoll = ev.now
      out.actions.push({ type: 'poll-closed', slug: t.slug, pr: t.pr })
    }
    if (t.stage === 'checking' && t.pr !== undefined) {
      out.actions.push({ type: 'poll', slug: t.slug, pr: t.pr })
    }
    if (t.retryAt !== undefined && ev.now >= t.retryAt && t.retryAgent !== undefined && isLive(t)) {
      t.retryAt = undefined
      note(t, ev.now, 'resumed after an API error')
      tell(t, t.retryAgent, withPending(t, P.retryMessage()), out, 'send')
    }
  }
  startWaiting(s, ev.now, out, cfg)
}

/** The session started: wake what the store says is going; an agent the engine does not know is dead. */
function onRestore(s: State, ev: Event_<'restore'>, out: Result, cfg: Config): void {
  for (const t of s.tasks) {
    const dead = (id?: string): boolean => id !== undefined && agentOf(t, id)?.running === true && !ev.agents.includes(id)
    const developerDead = dead(t.agentId)
    const reviewerDead = dead(t.reviewerId)
    for (const a of t.agents) a.running = false
    switch (t.stage) {
      case 'developing':
        note(t, ev.now, 'the pipeline restarted')
        if (developerDead && t.agentId !== undefined) replaceDeveloper(t, t.agentId, P.restartMessage(), out)
        else toDeveloper(t, withPending(t, P.restartMessage()), out, cfg)
        break
      case 'reviewing':
        if (t.reviewerId !== undefined && !reviewerDead) tell(t, t.reviewerId, P.restartMessage(), out, 'send')
        else spawnReviewer(t, out, cfg)
        break
      case 'checking':
        if (t.pr !== undefined) out.actions.push({ type: 'poll', slug: t.slug, pr: t.pr })
        break
      case 'merging':
        // The merge may or may not have happened: read the review and the status again.
        t.stage = 'reviewing'
        if (t.pr !== undefined) out.actions.push({ type: 'read-review', slug: t.slug, pr: t.pr })
        break
      case 'merged':
        // Merged, but the session ended before the cleanup and the report: do them.
        if (t.lastEvent === 'merged; cleaning up' && t.pr !== undefined && t.head !== undefined) {
          out.actions.push({ type: 'post-merge', slug: t.slug, pr: t.pr, head: t.head })
        }
        break
      case 'blocked':
        maybeCleanup(t, out)
        break
      default:
    }
  }
  startWaiting(s, ev.now, out, cfg)
}

/** The worktree of a dead developer is saved and freed; the replacement is told where the unsaved work is. */
function onSalvaged(s: State, ev: Event_<'salvaged'>, out: Result, cfg: Config): void {
  const t = find(s, ev.slug)
  if (!t || t.stage !== 'developing' || t.agentId !== undefined) return
  const lines = [ev.text.trim()]
  if (ev.patch !== undefined) lines.push(P.salvagedPatch(ev.patch))
  if (ev.note !== undefined && ev.note !== '') lines.push(P.salvageNote(ev.note))
  spawnDeveloper(t, P.continuePrompt(t, lines.join('\n\n'), cfg), out)
}
