// config.ts: the project layer, `.claude/pipeline.json`, and the texts that use it.
import { describe, expect, test } from 'claude-code/testing'

import { type Config, DEFAULT_PROJECT, DEFAULT_TIMEOUT_SECONDS, needsAfterMerge, parseConfig, pullsUrl } from '../hooks/config'
import { continuePrompt, contract, developerPrompt, projectLine, reviewerPrompt } from '../hooks/prompts'
import { emptyState, step } from '../hooks/machine'
import { TEST_CONFIG } from './helpers'

const FORGE = { url: 'http://forge.test', repo: 'acme/widgets' }

function parsed(raw: unknown): Config {
  const r = parseConfig(JSON.stringify(raw))
  if (!r.ok) throw new Error(r.error)
  return r.config
}

function error(raw: unknown): string {
  const r = parseConfig(typeof raw === 'string' ? raw : JSON.stringify(raw))
  if (r.ok) throw new Error('parsed')
  return r.error
}

describe('parsing', () => {
  test('every key', async () => {
    const c = parsed({
      project: 'widgets',
      guide: 'docs/pipeline.md',
      map: 'docs/rules.md',
      forge: FORGE,
      afterMerge: { paths: ['src/'], command: ['scripts/after.sh', '-q'], timeoutSeconds: 30 },
      targetedTests: 'make test-one',
    })
    expect(c).toEqual({
      project: 'widgets',
      guide: 'docs/pipeline.md',
      map: 'docs/rules.md',
      forge: FORGE,
      afterMerge: { paths: ['src/'], command: ['scripts/after.sh', '-q'], timeoutSeconds: 30 },
      targetedTests: 'make test-one',
    })
  })

  test('only the forge is required; the rest has defaults', async () => {
    const c = parsed({ forge: FORGE })
    expect(c.project).toBe(DEFAULT_PROJECT)
    expect(c.guide).toBeUndefined()
    expect(c.map).toBeUndefined()
    expect(c.afterMerge).toBeUndefined()
    expect(c.targetedTests).toBeUndefined()
  })

  test('afterMerge: the timeout and the paths have defaults, the command does not', async () => {
    const c = parsed({ forge: FORGE, afterMerge: { command: ['run'] } })
    expect(c.afterMerge).toEqual({ paths: [], command: ['run'], timeoutSeconds: DEFAULT_TIMEOUT_SECONDS })
    expect(error({ forge: FORGE, afterMerge: { paths: ['src/'] } })).toContain('"afterMerge.command" is required')
  })

  test('unknown keys are ignored', async () => {
    const c = parsed({ forge: { ...FORGE, token: 'x' }, future: { a: 1 }, afterMerge: { command: ['run'], later: true } })
    expect(c.forge).toEqual(FORGE)
  })

  test('a trailing slash of the forge url is dropped', async () => {
    expect(parsed({ forge: { url: 'http://forge.test//', repo: 'acme/widgets' } }).forge.url).toBe('http://forge.test')
  })

  test('the errors name the file and every problem', async () => {
    expect(error('{ not json')).toContain('.claude/pipeline.json is not valid JSON')
    expect(error('[]')).toContain('must be a JSON object')
    expect(error({})).toContain('"forge" is required')
    const many = error({ project: 3, guide: '', forge: { url: 'ftp://x', repo: 'nope' }, afterMerge: { command: [], timeoutSeconds: 0 } })
    for (const part of ['"project"', '"guide"', '"forge.url"', '"forge.repo"', '"afterMerge.command"', '"afterMerge.timeoutSeconds"']) {
      expect(many).toContain(part)
    }
    expect(error({ forge: FORGE, afterMerge: 'run' })).toContain('"afterMerge" must be an object')
    expect(error({ forge: FORGE, targetedTests: 7 })).toContain('"targetedTests"')
  })
})

describe('using it', () => {
  test('the pull request address comes from the forge', async () => {
    expect(pullsUrl(TEST_CONFIG)).toBe('http://forge.test/acme/widgets/pulls/')
  })

  test('the after-merge command is for the changes under its paths', async () => {
    const am = { paths: ['src/', 'lib/'], command: ['run'], timeoutSeconds: 1 }
    expect(needsAfterMerge(['docs/a.md', 'srcs/x'], am)).toBe(false)
    expect(needsAfterMerge(['docs/a.md', 'src/x.rs'], am)).toBe(true)
    expect(needsAfterMerge(['src/x.rs'], undefined)).toBe(false)
    expect(needsAfterMerge([], { ...am, paths: [] })).toBe(false)
    expect(needsAfterMerge(['anything'], { ...am, paths: [] })).toBe(true)
  })

  test('the developer contract shows the test hint, and leaves the parentheses out without one', async () => {
    expect(contract('feat', TEST_CONFIG)).toContain('the targeted tests your change needs (make test-one T=<name>), not the full check of the project')
    expect(contract('feat', { ...TEST_CONFIG, targetedTests: undefined })).toContain('the targeted tests your change needs, not the full check')
  })

  test('agents are told the project and what to read first', async () => {
    const line = projectLine(TEST_CONFIG)
    expect(line).toContain('widgets')
    expect(line).toContain('docs/pipeline.md')
    expect(line).toContain('Project specifics for the agents')
    expect(line).toContain('docs/rules.md')
    expect(projectLine({ ...TEST_CONFIG, guide: undefined, map: undefined })).toBe('Project: widgets (.claude/pipeline.json).')
    const run = step(emptyState(), { type: 'start', slug: 'feat', developer: 'developer-1', brief: 'Brief.', after: [], now: 1 }, TEST_CONFIG)
    const task = run.state.tasks[0]
    if (!task) throw new Error('no task')
    for (const prompt of [developerPrompt(task, TEST_CONFIG), continuePrompt(task, 'More.', TEST_CONFIG), reviewerPrompt(task, 4, 'abc', TEST_CONFIG)]) {
      expect(prompt).toContain(line)
    }
    const spawn = run.actions.find(a => a.type === 'spawn')
    expect(spawn?.type === 'spawn' && spawn.prompt).toContain('make test-one T=<name>')
  })
})
