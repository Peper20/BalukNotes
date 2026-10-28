// The code-skill guard: which files a tool call writes, and which skill the
// project requires before such an edit (the `skills` key, config.md). Pure: the
// loaded skills are kept and the call is denied by register.tsx.
import type { SkillRule } from './config'

/** The tools that write the file their input names, with the input key of the path. */
const FILE_TOOLS: Readonly<Record<string, string>> = {
  Edit: 'file_path',
  Write: 'file_path',
  MultiEdit: 'file_path',
  NotebookEdit: 'notebook_path',
}

/** Signs of writing a file in one part of a shell command (reading - grep, sed -n, cat - is not). */
const WRITES = /sed\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*i|perl\s+-[a-zA-Z]*i|\btee\b|\.write\(|write_text\(|open\([^)]*['"][wa]/

/** The target of a redirect: a write by itself. */
const REDIRECT = />>?\s*['"]?([^\s'"|;&<>]+)/g

/** What may be a path in a command. */
const WORD = /[^\s'"|;&<>()=,]+/g

/** Where one part of a shell command ends. */
const PARTS = /&&|\|\||[;|\n]/

/** The location of a `Skill` tool call or of a slash command without its `<plugin>:` prefix. */
export const skillName = (name: string): string => name.slice(name.lastIndexOf(':') + 1)

/** `path` made absolute against `cwd` and cleaned of `.`, `..` and repeated slashes; `~` is the home. */
function resolve(path: string, cwd: string, home: string | undefined): string {
  const expanded = home !== undefined && (path === '~' || path.startsWith('~/')) ? home + path.slice(1) : path
  const parts: string[] = []
  for (const part of (expanded.startsWith('/') ? expanded : `${cwd}/${expanded}`).split('/')) {
    if (part === '..') parts.pop()
    else if (part !== '' && part !== '.') parts.push(part)
  }
  return `/${parts.join('/')}`
}

/** The worktrees of the agents, under the project root: a path in one is taken against that worktree's own root. */
const WORKTREE = /^\.claude\/worktrees\/[^/]+\//

/** The path against the project root (or the root of the worktree it is in), or undefined when it is the root itself or outside it. */
function inProject(path: string, cwd: string, root: string, home: string | undefined): string | undefined {
  const full = resolve(path, cwd, home)
  const base = resolve(root, '/', home)
  if (!full.startsWith(`${base === '/' ? '' : base}/`)) return undefined
  return full.slice(base.length).replace(/^\//, '').replace(WORKTREE, '')
}

/**
 * The paths a shell command writes: it redirects into them, or a part of it both
 * writes and names them. Every word of a writing part (but an option) is a
 * candidate; the rules of the project tell which are files they care about.
 */
function bashWrites(command: string): string[] {
  const named = [...command.matchAll(REDIRECT)].map(m => m[1] ?? '')
  for (const part of command.split(PARTS)) {
    if (WRITES.test(part)) named.push(...(part.match(WORD) ?? []).filter(w => !w.startsWith('-')))
  }
  return named
}

/**
 * The files a tool call writes, against the project root (`root`): the file of
 * an `Edit`, `Write`, `MultiEdit` or `NotebookEdit`, or what a `Bash` command
 * writes. A command that writes one file and only mentions another (a document
 * about the code) names only the first. A path outside the project is left out.
 * Relative paths are taken from `cwd`, `~` from `home`.
 */
export function editedPaths(tool: string, input: Record<string, unknown>, cwd: string, root: string, home?: string): string[] {
  const key = FILE_TOOLS[tool]
  let named: string[] = []
  if (key !== undefined) named = typeof input[key] === 'string' ? [input[key]] : []
  else if (tool === 'Bash') named = bashWrites(typeof input.command === 'string' ? input.command : '')
  const inside = named.map(p => inProject(p, cwd, root, home))
  return [...new Set(inside.filter((p): p is string => p !== undefined && p !== ''))]
}

/** The rules, one for a skill, that some of the paths match. */
export function requiredSkills(paths: readonly string[], rules: readonly SkillRule[]): SkillRule[] {
  const found = new Map<string, SkillRule>()
  for (const rule of rules) {
    if (!found.has(rule.skill) && paths.some(p => new RegExp(rule.match).test(p))) found.set(rule.skill, rule)
  }
  return [...found.values()]
}

/** The text of the refusal: the skill to load, the project's rules to read. */
export function denyText(rule: SkillRule): string {
  const language = rule.skill.charAt(0).toUpperCase() + rule.skill.slice(1)
  const rules = rule.rules === undefined ? '' : ` and read ${rule.rules} (project rules)`
  return `${language} edits need the ${rule.skill} skill loaded (again after a context compaction): call Skill "${rule.skill}"${rules}, then retry the edit.`
}
