// opencode plugin: report busy/idle as OSC 9;4 terminal progress.
//
// opencode has no native busy signal: its terminal title is a static
// "OpenCode" or "OC | <session>", and it never sends OSC 9;4. This plugin
// turns opencode's own session.status events into the same OSC 9;4 sequence
// Claude Code sends, so tmux-ai-helper (via pipe-pane) shows ⏳ while any
// session is working and ✅ when all are idle. Terminals that understand
// OSC 9;4, such as Ghostty, show it too.
//
// Install: copy to ~/.config/opencode/plugins/tmux-progress.js and restart
// opencode. See the tmux-ai-helper README, "opencode Setup".

import { openSync, writeSync } from "node:fs"

let tty = null
try {
  tty = openSync("/dev/tty", "w")
} catch {
  // No controlling terminal (e.g. a headless server): nothing to report to.
}

const busy = new Set()
let last = null

function emit(state) {
  if (tty === null || state === last) return
  last = state
  try {
    writeSync(tty, `\x1b]9;4;${state};\x07`)
  } catch {}
}

// Do not leave the pane stuck on ⏳ if opencode exits mid-turn.
process.on("exit", () => {
  if (last === 3) emit(0)
})

export const TmuxProgress = async () => ({
  event: async ({ event }) => {
    if (event.type !== "session.status") return
    const { sessionID, status } = event.properties
    if (status?.type === "idle") busy.delete(sessionID)
    else busy.add(sessionID) // "busy" and "retry"
    emit(busy.size > 0 ? 3 : 0)
  },
})
