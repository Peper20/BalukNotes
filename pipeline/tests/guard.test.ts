// guard.ts and its wiring in register.tsx: which edits need a code skill, and when a loop has loaded it.
import type { On } from 'claude-code'
import { describe, type Engine, expect, mock, test } from 'claude-code/testing'

import { denyText, editedPaths, requiredSkills, skillName } from '../hooks/guard'

const ROOT = '/repo'
const RUST = { match: '\\.rs$', skill: 'rust', rules: '.claude/rules/crates.md' }

const bash = (command: string, cwd = ROOT): string[] => editedPaths('Bash', { command }, cwd, ROOT, '/home/u')
const edited = (tool: string, input: Record<string, unknown>): string[] => editedPaths(tool, input, ROOT, ROOT, '/home/u')

describe('the files a call writes', () => {
  test('the file tools: the path against the project root', () => {
    expect(edited('Edit', { file_path: '/repo/crates/x/src/a.rs' })).toEqual(['crates/x/src/a.rs'])
    expect(edited('Write', { file_path: '/repo/a.txt' })).toEqual(['a.txt'])
    expect(edited('MultiEdit', { file_path: '/repo/./b/../a.rs' })).toEqual(['a.rs'])
    expect(edited('NotebookEdit', { notebook_path: '/repo/n.ipynb' })).toEqual(['n.ipynb'])
    expect(edited('Edit', { file_path: 'src/a.rs' })).toEqual(['src/a.rs'])
  })

  test('a file outside the project, the project itself, no path and other tools: nothing', () => {
    expect(edited('Edit', { file_path: '/tmp/a.rs' })).toEqual([])
    expect(edited('Edit', { file_path: '/repository/a.rs' })).toEqual([])
    expect(edited('Edit', { file_path: '/repo' })).toEqual([])
    expect(edited('Edit', {})).toEqual([])
    expect(edited('Read', { file_path: '/repo/a.rs' })).toEqual([])
    expect(bash('sed -i s/a/b/ /tmp/a.rs').filter(p => p.endsWith('.rs'))).toEqual([])
    expect(bash('echo x > /tmp/a.rs')).toEqual([])
  })

  test('a command that writes: sed -i, perl -i, tee, a write from python', () => {
    expect(bash("sed -i 's/a/b/' a.rs")).toContain('a.rs')
    expect(bash("sed -n -i 's/a/b/' a.rs")).toContain('a.rs')
    expect(bash('perl -pi -e "s/a/b/" crates/a.rs')).toContain('crates/a.rs')
    expect(bash('echo hi | tee a.rs')).toContain('a.rs')
    expect(bash(`python3 -c "open('a.rs', 'w').write('x')"`)).toContain('a.rs')
    expect(bash('true && sed -i s/a/b/ a.rs')).toContain('a.rs')
  })

  test('a redirect into a file is a write by itself', () => {
    expect(bash('echo x > a.rs')).toEqual(['a.rs'])
    expect(bash('cat b >> "src/a.rs"')).toEqual(['src/a.rs'])
    expect(bash('echo x >/repo/a.rs')).toEqual(['a.rs'])
  })

  test('reading is not writing; a part that writes another file does not name the file it mentions', () => {
    expect(bash('grep foo a.rs')).toEqual([])
    expect(bash('sed -n 1,5p a.rs')).toEqual([])
    expect(bash('cat a.rs | wc -l')).toEqual([])
    expect(bash('grep foo a.rs > notes.md')).toEqual(['notes.md'])
    expect(bash('sed -i s/a/b/ notes.md; cat a.rs')).not.toContain('a.rs')
    expect(bash('cargo test 2>&1 | tail -5')).toEqual([])
  })

  test('a path in a worktree under the project is taken against the worktree: one pattern means the same in every checkout', () => {
    const wt = '/repo/.claude/worktrees/agent-1'
    expect(edited('Edit', { file_path: `${wt}/crates/x/src/a.rs` })).toEqual(['crates/x/src/a.rs'])
    expect(editedPaths('Bash', { command: 'echo x > crates/a.rs' }, wt, ROOT)).toEqual(['crates/a.rs'])
    expect(edited('Edit', { file_path: '/repo/.claude/rules/crates.md' })).toEqual(['.claude/rules/crates.md'])
    const anchored = { match: '^crates/.*\\.rs$', skill: 'rust' }
    for (const path of ['/repo/crates/x/src/a.rs', `${wt}/crates/x/src/a.rs`]) {
      expect(requiredSkills(edited('Edit', { file_path: path }), [anchored])).toEqual([anchored])
    }
    expect(requiredSkills(edited('Edit', { file_path: `${wt}/app/a.rs` }), [anchored])).toEqual([])
  })

  test('relative paths are taken from the cwd of the call; ~ from the home', () => {
    expect(bash('echo x > a.rs', '/repo/crates')).toEqual(['crates/a.rs'])
    expect(bash('echo x > ../a.rs', '/repo/crates')).toEqual(['a.rs'])
    expect(bash('echo x > a.rs', '/elsewhere')).toEqual([])
    expect(editedPaths('Bash', { command: 'echo x > ~/proj/a.rs' }, ROOT, '/home/u/proj', '/home/u')).toEqual(['a.rs'])
    expect(editedPaths('Bash', { command: 'echo x > ~/proj/a.rs' }, ROOT, '/home/u/proj')).toEqual([])
  })
})

describe('the skill a path needs', () => {
  const SVELTE = { match: '^app/.*\\.svelte$', skill: 'svelte' }

  test('one rule per skill, whichever paths match', () => {
    expect(requiredSkills(['a.rs'], [RUST, SVELTE])).toEqual([RUST])
    expect(requiredSkills(['a.rs', 'b.rs', 'app/x.svelte'], [RUST, SVELTE])).toEqual([RUST, SVELTE])
    expect(requiredSkills(['notes.md', 'app/x.ts'], [RUST, SVELTE])).toEqual([])
    expect(requiredSkills(['a.rs'], [])).toEqual([])
  })

  test('the refusal names the skill and the rules', () => {
    expect(denyText(RUST)).toBe(
      'Rust edits need the rust skill loaded (again after a context compaction): call Skill "rust" and read .claude/rules/crates.md (project rules), then retry the edit.',
    )
    expect(denyText(SVELTE)).toBe('Svelte edits need the svelte skill loaded (again after a context compaction): call Skill "svelte", then retry the edit.')
  })

  test('a skill name loses its plugin prefix', () => {
    expect(skillName('rust')).toBe('rust')
    expect(skillName('pipeline:rust')).toBe('rust')
  })
})

// --- through the engine ---------------------------------------------------------

/** A compaction leaves at least one message. */
const MESSAGES = [{ role: 'user' as const, text: 'summary', toolUses: [] }]

/** `skills` only: no forge, so the pipeline does not start, and the guard works all the same. */
const SKILLS_ONLY = JSON.stringify({ skills: [RUST] })

async function session($: Engine, on: On, file: string | null = SKILLS_ONLY) {
  mock.clock(on)
  mock.store(on)
  mock.env(on, { HOME: '/home/u' })
  const logs: string[] = []
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.root', async () => ({ value: ROOT }))
  on('session.cwd', async () => ({ value: ROOT }))
  on('fs.read', async () => {
    if (file === null) throw new Error('no such file')
    return { value: file }
  })
  on('tool.register', async (_$, e) => ({ value: { tool: `mcp__pipeline__${e.name}` } }))
  on('command.register', async () => ({ value: { command: 'pipeline' } }))
  on('ui.status', async () => ({ value: undefined }))
  on('ui.log', async (_$, e) => {
    logs.push(String(e.text))
    return { value: undefined }
  })
  on('agent.list', async () => ({ value: [] }))
  on('prompt.submit', async (_$, e) => ({ text: e.text }))
  on('tool.call', async (_$, e) => (e.tool === 'Skill' && e.skill === 'nonsense' ? { deny: 'no such skill' } : { result: 'done' as never }))
  on('session.compact', async () => ({ messages: MESSAGES }))
  on('command.run', async () => ({ text: '' }))
  await $.session.start({ cwd: ROOT, surface: 'terminal', isInteractive: true })
  return { logs }
}

const edit = (agentId?: string, path = '/repo/crates/x/src/a.rs') => ({ tool: 'Edit', file_path: path, old_string: 'a', new_string: 'b', ...(agentId ? { agentId } : {}) }) as never
const skill = (name: string, agentId?: string) => ({ tool: 'Skill', skill: name, ...(agentId ? { agentId } : {}) }) as never
const sh = (command: string, agentId?: string) => ({ tool: 'Bash', command, ...(agentId ? { agentId } : {}) }) as never
const compact = (agentId?: string) => ({ trigger: 'auto', messages: MESSAGES, ...(agentId ? { agentId } : {}) }) as never

test('an edit is denied before the skill, allowed after it, denied again after a compaction', async ($, on) => {
  await session($, on)
  const denied = await $.tool.call(edit())
  expect(denied.deny).toContain('Rust edits need the rust skill loaded')
  expect(denied.deny).toContain('.claude/rules/crates.md')

  expect((await $.tool.call(skill('rust'))).deny).toBeUndefined()
  expect((await $.tool.call(edit())).deny).toBeUndefined()

  await $.session.compact(compact())
  expect((await $.tool.call(edit())).deny).toContain('Rust edits need the rust skill loaded')

  await $.tool.call(skill('pipeline:rust'))
  expect((await $.tool.call(edit())).deny).toBeUndefined()
})

test('a loop has its own set: a subagent\'s load does not count for the main loop, nor the reverse', async ($, on) => {
  await session($, on)
  await $.tool.call(skill('rust', 'dev-1'))
  expect((await $.tool.call(edit('dev-1'))).deny).toBeUndefined()
  expect((await $.tool.call(edit())).deny).toContain('rust skill')
  expect((await $.tool.call(edit('dev-2'))).deny).toContain('rust skill')

  await $.tool.call(skill('rust'))
  expect((await $.tool.call(edit())).deny).toBeUndefined()
  expect((await $.tool.call(edit('dev-2'))).deny).toContain('rust skill')

  await $.session.compact(compact('dev-1'))
  expect((await $.tool.call(edit('dev-1'))).deny).toContain('rust skill')
  expect((await $.tool.call(edit())).deny).toBeUndefined()
})

test('a precompute installs no compaction and clears nothing', async ($, on) => {
  await session($, on)
  await $.tool.call(skill('rust'))
  await $.session.compact({ trigger: 'precompute', messages: MESSAGES } as never)
  expect((await $.tool.call(edit())).deny).toBeUndefined()
})

test('a Skill call that fails does not count', async ($, on) => {
  await session($, on)
  await $.tool.call(skill('nonsense'))
  await $.tool.call(skill('python'))
  expect((await $.tool.call(edit())).deny).toContain('rust skill')
})

test('the /<skill> command of the person counts for the main loop', async ($, on) => {
  await session($, on)
  const typed = (command: string) => ({ command, args: '', origin: { kind: 'composer' }, presentation: { layout: 'main', columns: 80 } }) as never
  await $.command.run(typed('compact'))
  expect((await $.tool.call(edit())).deny).toContain('rust skill')
  await $.command.run(typed('rust'))
  expect((await $.tool.call(edit())).deny).toBeUndefined()
  expect((await $.tool.call(edit('dev-1'))).deny).toContain('rust skill')
})

test('a write by a shell command is guarded like an edit', async ($, on) => {
  await session($, on)
  for (const command of ["sed -i 's/a/b/' crates/x/src/a.rs", 'echo x > crates/x/src/a.rs', 'echo x | tee crates/x/src/a.rs']) {
    expect((await $.tool.call(sh(command))).deny).toContain('rust skill')
  }
  expect((await $.tool.call(sh('grep foo crates/x/src/a.rs > notes.md'))).deny).toBeUndefined()
  expect((await $.tool.call(sh('cargo test'))).deny).toBeUndefined()
  expect((await $.tool.call(edit(undefined, '/tmp/a.rs'))).deny).toBeUndefined()
  expect((await $.tool.call(edit(undefined, '/repo/README.md'))).deny).toBeUndefined()
  await $.tool.call(skill('rust'))
  expect((await $.tool.call(sh('echo x > crates/x/src/a.rs'))).deny).toBeUndefined()
})

test('without the skills key, or without the file, nothing is denied', async ($, on) => {
  await session($, on, JSON.stringify({ project: 'widgets' }))
  expect((await $.tool.call(edit())).deny).toBeUndefined()
})

test('a project without the file: nothing is denied', async ($, on) => {
  await session($, on, null)
  expect((await $.tool.call(edit())).deny).toBeUndefined()
})

test('a bad rule is logged once and guards nothing until the file is fixed', async ($, on) => {
  const { logs } = await session($, on, JSON.stringify({ skills: [{ match: '(', skill: 'rust' }] }))
  expect((await $.tool.call(edit())).deny).toBeUndefined()
  expect((await $.tool.call(edit())).deny).toBeUndefined()
  expect(logs.filter(l => l.includes('skill guard'))).toHaveLength(1)
  expect(logs.join('\n')).toContain('not a valid regular expression')
})
