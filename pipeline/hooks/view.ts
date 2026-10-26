// What the liaison and the pane see of the tasks.
import type { PipelineViewTask } from '../types'
import { type Config, pullsUrl } from './config'
import { DEVELOPERS, type Stage, type State, type Task } from './model'

const STAGES: readonly Stage[] = ['waiting', 'developing', 'liaison', 'checking', 'reviewing', 'merging', 'blocked', 'merged']

const roundsOf = (t: Task): string => DEVELOPERS.map(d => `${d.replace('developer-', 'd')}:${t.rounds[d]}`).join(' ')

/** One task as the `pipeline_status` tool shows it. */
export function describeTask(t: Task, cfg: Config): string {
  const parts = [
    `${t.slug}: ${t.stage}`,
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

export function describeAll(s: State, cfg: Config): string {
  if (s.tasks.length === 0) return 'No tasks.'
  return s.tasks.map(t => describeTask(t, cfg)).join('\n')
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
