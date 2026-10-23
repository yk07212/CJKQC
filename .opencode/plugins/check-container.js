// Riprap-managed required-location adapter. Do not edit; use copier update.
// Locally adapted to the OpenCode V2 plugin API: OpenCode >= 2 requires a
// default export with an id and a setup function (the shipped V1 form — a
// named export returning hooks — fails to load and logs a PluginLoadError on
// every reload).

import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'

const message =
  'Riprap: OpenCode must run inside the project development container. ' +
  'Launch it with "bash rr.sh" (or "rr.bat" on Windows).'

const CHECK_SCRIPT = '.riprap/managed/hooks/check-container.sh'

// OpenCode derives `worktree` from git, so it is empty whenever git cannot
// resolve one: a checkout that was never `git init`-ed, a bare repository, or a
// bind-mounted repository whose ownership git refuses as "dubious". None of
// those say anything about whether we are containerized, yet keying the
// guardrail on `worktree` alone refuses every prompt in them even inside the
// container. Locate the managed check from whichever project root OpenCode does
// provide, falling back to the working directory, so the boundary tracks the
// real container check rather than git state.
function locateCheck(roots) {
  for (const root of roots) {
    if (!root) continue
    const script = join(root, CHECK_SCRIPT)
    if (existsSync(script)) return script
  }
  return undefined
}

// Fail closed: with no managed check to consult, refuse rather than assume a
// container. On a host the check is found and reports failure, so the refusal
// is the same either way. Throwing from the prompt hook stops admission, which
// is the V2 equivalent of the V1 chat.message refusal.
function rejectOutsideContainer(roots) {
  if (process.platform === 'win32') throw new Error(message)

  const script = locateCheck(roots)
  if (!script) throw new Error(message)

  let result
  try {
    result = spawnSync('bash', [script], { encoding: 'utf8' })
  } catch {
    throw new Error(message)
  }
  if (!result || result.error || result.status !== 0) throw new Error(message)
}

export default {
  id: 'riprap-container-check',
  async setup(ctx) {
    const roots = [
      ctx?.location?.directory,
      ctx?.location?.project?.directory,
      process.cwd(),
    ]
    await ctx.session.hook('prompt', () => {
      rejectOutsideContainer(roots)
    })
  },
}
