// The values the pipeline mod keeps in `$.state`: the rows of its pane and the
// skills each loop has loaded (the guard, hooks/guard.ts).
export type PipelineViewTask = {
  slug: string
  stage: string
  /** 0: no pull request yet. */
  pr: number
  ci: string
  developer: string
  /** `d1:0 d2:1 d3:0`. */
  rounds: string
  /** The last event of the task. */
  line: string
}

declare module 'claude-code' {
  interface PluginState {
    pipeline: {
      tasks: PipelineViewTask[]
      started: boolean
      /** Per loop (its agent id, or `main`): the names of the skills loaded since its last compaction. */
      skills: StateFamily<string[]>
    }
  }
}
