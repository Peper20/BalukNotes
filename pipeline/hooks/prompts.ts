// The texts the pipeline sends: to developers, reviewers and the liaison.
import { CONFIG_PATH, type Config } from './config'
import { type Ending, MAX_ROUNDS, type Task } from './model'

export const branchOf = (slug: string): string => `task/${slug}`

const ENDING_LINE = /^PIPELINE:\s*(?:pr=(\d+)(?:\s+(liaison))?|(question))\s*$/

/** The `PIPELINE:` line a developer ends its final message with (the last one wins). */
export function parseEnding(text: string): Ending | undefined {
  const lines = text.split('\n').map(line => line.trim())
  for (let i = lines.length - 1; i >= 0; i--) {
    const m = ENDING_LINE.exec(lines[i] ?? '')
    if (!m) continue
    if (m[3] !== undefined) return { kind: 'question' }
    return { kind: 'pr', pr: Number(m[1]), liaison: m[2] !== undefined }
  }
  return undefined
}

/** What the project layer tells an agent to read first, or nothing when the file names no page. */
export function projectLine(cfg: Config): string {
  const pages = [cfg.guide && `its pipeline guide ${cfg.guide} (the section "Project specifics for the agents")`, cfg.map && `its rules map ${cfg.map}`]
    .filter(p => p !== undefined && p !== '')
    .join(' and ')
  return `Project: ${cfg.project} (${CONFIG_PATH}).${pages === '' ? '' : ` Read first: ${pages}.`}`
}

export const contract = (slug: string, cfg: Config): string =>
  [
    'Pipeline contract (an automated pipeline reads your last message):',
    `- Work on the branch ${branchOf(slug)}. Run the targeted tests your change needs${cfg.targetedTests === undefined ? '' : ` (${cfg.targetedTests})`}, not the full check of the project: the full check runs on the pull request.`,
    '- Push the branch and open the pull request (or update the existing one), as the dev-workflow skill says. Never merge.',
    '- End your final message with exactly one last line:',
    '  PIPELINE: pr=<number>            the pull request is ready for the full check and review',
    '  PIPELINE: pr=<number> liaison    your report holds architecture additions, deferred items or a question that the liaison must see before the review',
    '  PIPELINE: question               no pull request: you stopped on a question or a contradiction',
    '- When you are resumed with findings, a failed check or a message, work on it and end the same way.',
  ].join('\n')

export function developerPrompt(t: Task, cfg: Config): string {
  return [t.brief.trim(), '', `Branch: ${branchOf(t.slug)}.`, projectLine(cfg), '', contract(t.slug, cfg)].join('\n')
}

/** A developer that continues a task whose branch (and pull request) already exist. */
export function continuePrompt(t: Task, text: string, cfg: Config): string {
  const pr = t.pr === undefined ? 'The pull request may not exist yet: check the server.' : `The pull request is #${t.pr}.`
  return [
    t.brief.trim(),
    '',
    `The branch ${branchOf(t.slug)} already exists on the server and you continue the task of an earlier developer. ${pr}`,
    `Do not create the branch: get it as the forge skill says (git fetch origin ${branchOf(t.slug)}, then git switch ${branchOf(t.slug)}), read the pull request with its comments, reviews and commits, and continue from the existing commits. Push to the same branch; do not open another pull request.`,
    projectLine(cfg),
    '',
    text.trim(),
    '',
    contract(t.slug, cfg),
  ].join('\n')
}

export function reviewerPrompt(t: Task, pr: number, head: string, cfg: Config): string {
  return [
    `Review pull request #${pr} (task ${t.slug}, branch ${branchOf(t.slug)}, head ${head}).`,
    projectLine(cfg),
    'The full check of this head has passed. The brief of the task, with its acceptance criteria and design:',
    '',
    t.brief.trim(),
  ].join('\n')
}

const round = (n: number): string => `round ${n} of ${MAX_ROUNDS} for this developer`

export function checkFailedMessage(t: Task, n: number, detail: string): string {
  return [
    `The full check of pull request #${t.pr} failed at ${t.head ?? 'its head'} (${round(n)}).`,
    'Fix it, push, and end your final message with the PIPELINE line again. If the failure is not caused by your change, say so with the evidence.',
    '',
    detail.trim(),
  ].join('\n')
}

export function changesMessage(t: Task, n: number, review: string): string {
  return [
    `The reviewer requested changes on pull request #${t.pr} (${round(n)}).`,
    'Fix the blocking findings, push, answer each finding in a pull request comment, and end your final message with the PIPELINE line again.',
    '',
    review.trim(),
  ].join('\n')
}

export function rebaseMessage(t: Task): string {
  return `Pull request #${t.pr} conflicts with main since another pull request was merged. Rebase ${branchOf(t.slug)} onto origin/main (git fetch origin, then git rebase origin/main), resolve the conflicts, run the targeted tests, push. A rebase rewrites the branch, so push it with git push --force-with-lease: this is allowed only to this task's own branch ${branchOf(t.slug)}, never to main or to another branch. If that push is refused, merge origin/main into the branch instead and push normally. End your final message with the PIPELINE line again.`
}

export const nudgeMessage = (): string =>
  'Your turn ended without the PIPELINE line. Finish: end your final message with exactly one last line, PIPELINE: pr=<number>, PIPELINE: pr=<number> liaison or PIPELINE: question.'

export const retryMessage = (): string =>
  'Your previous turn was cut by an API error (a usage limit). Continue where you stopped and end your final message with the PIPELINE line.'

export const restartMessage = (): string =>
  'The pipeline was restarted. Continue where you stopped and end your final message with the PIPELINE line.'

export const stopMessage = (reason: string): string =>
  `The user stopped this task: ${reason}. Commit and push to the task branch what is worth keeping (open a pull request only if there is something worth keeping and none exists), then stop. Do not start anything new.`

export function liaisonEvent(t: Task, why: string): string {
  const pr = t.pr === undefined ? 'no pull request' : `pull request #${t.pr}`
  return `[pipeline] ${t.slug} (${pr}): ${why}`
}

export function blockedComment(t: Task, reason: string, findings: string): string {
  return [`Blocked by the pipeline: ${reason}.`, findings.trim() === '' ? '' : `Open findings:\n\n${findings.trim()}`]
    .filter(line => line !== '')
    .join('\n\n')
}

export const lateSendMessage = (text: string): string =>
  `A message from the liaison that came while your last turn was ending (ignore it if you already acted on it):\n\n${text.trim()}`

/** A message of the liaison that a developer which is gone may not have read. */
export const lostMessage = (text: string): string =>
  `A message from the liaison that the previous developer may not have read:\n\n${text.trim()}`

export const salvagedPatch = (path: string): string =>
  `The previous developer's unsaved changes are in ${path}: apply what is worth keeping.`

export const salvageNote = (note: string): string => `The previous developer's worktree could not be freed (${note}); its unsaved changes may still be there.`

export const orphanWorktrees = (paths: readonly string[]): string =>
  `[pipeline] worktrees that no task of the pipeline references (not removed; look at them and remove by hand what is not needed):\n- ${paths.join('\n- ')}`

export const heldMessage = (text: string): string =>
  `A message from the liaison, sent while the pipeline was paused:\n\n${text.trim()}`

export function sessionPaused(unfinished: readonly string[], orphans: readonly string[]): string {
  return [
    '[pipeline] the session started; the pipeline is paused.',
    `Unfinished:\n- ${unfinished.join('\n- ')}`,
    orphans.length === 0 ? '' : `Unreferenced worktrees (not removed; look at them and remove by hand what is not needed):\n- ${orphans.join('\n- ')}`,
    'Call pipeline_resume after the user agrees.',
  ]
    .filter(line => line !== '')
    .join('\n')
}
