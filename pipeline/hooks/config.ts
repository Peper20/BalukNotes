// The project layer of the mod: `.claude/pipeline.json` of the project the session
// runs in (reference: config.md of the kit). Parsed once at session start; unknown
// keys are ignored, so a newer file works with an older mod. Pure: the file is
// read by register.tsx.

export const CONFIG_PATH = '.claude/pipeline.json'

export type AfterMerge = {
  /** Path prefixes: a merge that changes a file under one runs the command. Empty: any change. */
  paths: string[]
  /** An argument vector, run in the main checkout. */
  command: string[]
  timeoutSeconds: number
}

/** The check of the project, run by the kit's scripts (config.md); the mod reads only `logs`. */
export type Check = {
  steps?: string
  env?: string
  logs?: string
  heavyLock?: string
}

export type Config = {
  /** The project's name, for texts. */
  project: string
  /** The project's pipeline page that agents read for project specifics. */
  guide?: string
  /** The project's map of rules. */
  map?: string
  forge: { url: string; repo: string }
  afterMerge?: AfterMerge
  check?: Check
  /** The hint on the targeted tests that the developer contract shows. */
  targetedTests?: string
}

export type ConfigResult = { ok: true; config: Config } | { ok: false; error: string }

export const DEFAULT_PROJECT = 'the project'
export const DEFAULT_TIMEOUT_SECONDS = 600
/** Where the check writes its logs when `check.logs` is not set (the default of pipeline/scripts/check.sh). */
export const DEFAULT_CHECK_LOGS = '.pipeline/check'

/** The folder of the check logs against the project root. */
export const checkLogs = (c: Config): string => (c.check?.logs ?? DEFAULT_CHECK_LOGS).replace(/\/+$/, '')

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v)
const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every(x => typeof x === 'string' && x !== '')

/** `.claude/pipeline.json` as text to a config, or every problem found in it. */
export function parseConfig(text: string): ConfigResult {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch (err) {
    return { ok: false, error: `${CONFIG_PATH} is not valid JSON: ${err instanceof Error ? err.message : String(err)}` }
  }
  if (!isRecord(raw)) return { ok: false, error: `${CONFIG_PATH} must be a JSON object` }

  const problems: string[] = []
  const optionalString = (key: 'guide' | 'map' | 'targetedTests'): string | undefined => {
    const v = raw[key]
    if (v === undefined) return undefined
    if (typeof v === 'string' && v.trim() !== '') return v.trim()
    problems.push(`"${key}" must be a non-empty string`)
    return undefined
  }

  let project = DEFAULT_PROJECT
  if (raw.project !== undefined) {
    if (typeof raw.project === 'string' && raw.project.trim() !== '') project = raw.project.trim()
    else problems.push('"project" must be a non-empty string')
  }

  let forge: Config['forge'] | undefined
  if (!isRecord(raw.forge)) {
    problems.push('"forge" is required: { "url": "http://...", "repo": "owner/name" }')
  } else {
    const { url, repo } = raw.forge
    const urlOk = typeof url === 'string' && /^https?:\/\/[^\s/]+\S*$/.test(url)
    const repoOk = typeof repo === 'string' && /^[^\s/]+\/[^\s/]+$/.test(repo)
    if (!urlOk) problems.push('"forge.url" must be an http(s) URL')
    if (!repoOk) problems.push('"forge.repo" must be "owner/name"')
    if (urlOk && repoOk) forge = { url: url.replace(/\/+$/, ''), repo }
  }

  let afterMerge: AfterMerge | undefined
  if (raw.afterMerge !== undefined) {
    const am = raw.afterMerge
    if (!isRecord(am)) {
      problems.push('"afterMerge" must be an object: { "paths": [...], "command": [...], "timeoutSeconds": 600 }')
    } else {
      const paths = am.paths === undefined ? [] : am.paths
      const timeout = am.timeoutSeconds === undefined ? DEFAULT_TIMEOUT_SECONDS : am.timeoutSeconds
      const pathsOk = isStrings(paths)
      const commandOk = isStrings(am.command) && am.command.length > 0
      const timeoutOk = typeof timeout === 'number' && Number.isFinite(timeout) && timeout > 0
      if (!pathsOk) problems.push('"afterMerge.paths" must be a list of path prefixes')
      if (!commandOk) problems.push('"afterMerge.command" is required: a non-empty list, the program and its arguments')
      if (!timeoutOk) problems.push('"afterMerge.timeoutSeconds" must be a positive number')
      if (pathsOk && commandOk && timeoutOk) afterMerge = { paths, command: am.command as string[], timeoutSeconds: timeout }
    }
  }

  let check: Check | undefined
  if (raw.check !== undefined) {
    if (!isRecord(raw.check)) {
      problems.push('"check" must be an object: { "steps": "...", "env": "...", "logs": "...", "heavyLock": "..." }')
    } else {
      check = {}
      for (const key of ['steps', 'env', 'logs', 'heavyLock'] as const) {
        const v = raw.check[key]
        if (v === undefined) continue
        if (typeof v === 'string' && v.trim() !== '') check[key] = v.trim()
        else problems.push(`"check.${key}" must be a non-empty string`)
      }
    }
  }

  // Read by the kit's test script; checked here so that a mistake shows at session start.
  if (raw.tests !== undefined) {
    const ruleOk = (t: unknown): boolean =>
      isRecord(t) && typeof t.match === 'string' && t.match !== '' && typeof t.run === 'string' && t.run !== ''
    if (!Array.isArray(raw.tests) || !raw.tests.every(ruleOk)) {
      problems.push('"tests" must be a list of { "match": "<regex on the path>", "run": "<command>" }')
    }
  }

  const guide = optionalString('guide')
  const map = optionalString('map')
  const targetedTests = optionalString('targetedTests')

  if (problems.length > 0 || forge === undefined) {
    return { ok: false, error: `${CONFIG_PATH}: ${problems.join('; ')}` }
  }
  return { ok: true, config: { project, guide, map, forge, afterMerge, check, targetedTests } }
}

/** The pull request page of the forge, without the number. */
export const pullsUrl = (c: Config): string => `${c.forge.url}/${c.forge.repo}/pulls/`

/** Whether a merged change runs the command of `afterMerge`. */
export function needsAfterMerge(paths: readonly string[], am: AfterMerge | undefined): boolean {
  if (am === undefined) return false
  return am.paths.length === 0 ? paths.length > 0 : paths.some(p => am.paths.some(prefix => p.startsWith(prefix)))
}
