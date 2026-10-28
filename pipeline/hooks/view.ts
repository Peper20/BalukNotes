// What the liaison and the pane see of the tasks.
import type { PipelineViewTask } from '../types'
import { type Config, pullsUrl } from './config'
import { DEVELOPERS, SHOW_MERGED, STALE_MS, type Stage, type State, type Task } from './model'

const STAGES: readonly Stage[] = ['waiting', 'developing', 'liaison', 'checking', 'reviewing', 'merging', 'blocked', 'merged']

const roundsOf = (t: Task): string => DEVELOPERS.map(d => `${d.replace('developer-', 'd')}:${t.rounds[d]}`).join(' ')

/** One task as the `pipeline_status` tool shows it; `now` marks a blocked task nobody touched for long as stale. */
export function describeTask(t: Task, cfg: Config, now?: number): string {
  const stale = now !== undefined && t.stage === 'blocked' && now - t.lastAt > STALE_MS
  const parts = [
    `${t.slug}: ${t.stage}${stale ? ' stale' : ''}`,
    t.pr === undefined ? 'no PR' : `PR #${t.pr} (${pullsUrl(cfg)}${t.pr})`,
    `head ${t.head === undefined ? '-' : t.head.slice(0, 7)}`,
    `CI ${t.ci}`,
    `developer ${t.developer}${t.agentId === undefined ? '' : ` (agent ${t.agentId})`}`,
    `rounds ${roundsOf(t)}`,
  ]
  if (t.reviewerId !== undefined && t.stage === 'reviewing') parts.push(`reviewer agent ${t.reviewerId}`)
  if (t.after.length > 0 && t.stage === 'waiting') parts.push(`after ${t.after.join(', ')}`)
  if (t.blockedReason !== undefined && t.stage === 'blocked') parts.push(`reason: ${t.blockedReason}`)
  parts.push(`last: ${t.lastEvent}`)
  return parts.join(' | ')
}

/** Every task that is not merged, then the last merged ones (all of them with `all`), and how many are left out. */
export function describeAll(s: State, cfg: Config, now: number, all = false): string {
  if (s.tasks.length === 0) return 'No tasks.'
  const mergedAt = (t: Task): number => t.mergedAt ?? t.lastAt
  const merged = s.tasks.filter(t => t.stage === 'merged').sort((a, b) => mergedAt(b) - mergedAt(a))
  const shown = new Set(all ? merged : merged.slice(0, SHOW_MERGED))
  const lines = s.tasks.filter(t => t.stage !== 'merged').map(t => describeTask(t, cfg, now))
  lines.push(...merged.filter(t => shown.has(t)).reverse().map(t => describeTask(t, cfg, now)))
  const older = merged.length - shown.size
  if (older > 0) lines.push(`${older} older merged tasks: pipeline_status { all: true }`)
  return lines.join('\n')
}

/** The status line: the counts of tasks by stage, merged ones left out. The engine puts the plugin's name before it. */
export function summary(s: State): string | undefined {
  const counts = new Map<Stage, number>()
  for (const t of s.tasks) if (t.stage !== 'merged') counts.set(t.stage, (counts.get(t.stage) ?? 0) + 1)
  const parts = STAGES.filter(st => counts.has(st)).map(st => `${counts.get(st)} ${st}`)
  return parts.length === 0 ? undefined : parts.join(' · ')
}

/** The rows the pane draws (kept in `$.state`). */
export function viewRows(s: State): PipelineViewTask[] {
  return s.tasks.map(t => ({
    slug: t.slug,
    stage: t.stage,
    pr: t.pr ?? 0,
    ci: t.ci,
    developer: t.developer,
    rounds: roundsOf(t),
    line: t.lastEvent,
  }))
}
