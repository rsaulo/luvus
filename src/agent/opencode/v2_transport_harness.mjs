// Test harness for the OpenCode V2 plugin transport. It loads the real
// installed asset, drives it with a fake plugin context, and answers on a real
// local socket, so retry, precedence and release behavior are observed the way
// the CLI would produce it. Scenario name comes from argv; the received
// requests are printed as one JSON line.
import net from "node:net"
import plugin from "./tui.js"

const scenario = process.argv[2]
const address = process.env.LUVUS_API_ADDRESS
let failures = Number(process.env.HARNESS_FAILURES ?? 0)
const received = []
let onRequest = () => {}

const server = net.createServer((socket) => {
  socket.on("error", () => {})
  socket.on("data", (chunk) => {
    for (const line of chunk.toString("utf8").split("\n").filter(Boolean)) {
      const request = JSON.parse(line)
      received.push({ method: request.method, params: request.params })
      // A transient transport failure: the peer goes away mid-request.
      if (failures > 0) {
        failures -= 1
        socket.destroy()
      } else {
        socket.write(`${JSON.stringify({ id: request.id, result: { type: "ok" } })}\n`)
      }
      onRequest(received.length)
    }
  })
})
await new Promise((resolve) => server.listen(address, resolve))

// 2.0.3 publishes the session directory under `location`; 2.0.2 kept it flat,
// so `ses_legacy` pins the fallback.
const sessions = new Map([
  ["ses_a", { id: "ses_a", location: { directory: "/work" } }],
  ["ses_b", { id: "ses_b", location: { directory: "/work" } }],
  ["ses_legacy", { id: "ses_legacy", directory: "/work" }],
  ["ses_child", { id: "ses_child", location: { directory: "/work" }, parentID: "ses_a" }],
  ["ses_elsewhere", { id: "ses_elsewhere", location: { directory: "/other" } }],
])
let route = { type: "session", sessionID: process.env.HARNESS_ROUTE ?? "ses_a" }
const updated = []
const context = {
  location: { directory: "/work" },
  ui: {
    router: { current: () => route },
    slot: ({ render }) => {
      render()
      return () => {}
    },
  },
  data: {
    session: { get: (id) => sessions.get(id) },
    location: { default: () => ({ directory: "/work" }) },
    on: (_event, listener) => {
      updated.push(listener)
      return () => {}
    },
  },
}

// `globalThis.__luvusEffects` is filled by the solid-js stub this harness is
// installed next to; re-running them stands in for a reactive route read.
const navigate = (sessionID) => {
  route = sessionID ? { type: "session", sessionID } : { type: "home" }
  for (const effect of globalThis.__luvusEffects) effect()
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const waitFor = async (count) => {
  const deadline = Date.now() + 5000
  while (received.length < count && Date.now() < deadline) await sleep(10)
}
// Long enough for one bounded retry delay plus a request, so a stray extra
// request would show up in the transcript instead of being missed.
const settle = () => sleep(600)

const teardown = plugin.setup(context)

if (scenario === "retry") {
  // One failed request, then the same report must reach the server again.
  await waitFor(2)
  await settle()
} else if (scenario === "newer-report-wins") {
  // The user moves to another session while the first report is failing. The
  // newer report replaces the old one and keeps its own retry budget.
  onRequest = (count) => {
    if (count === 1) navigate("ses_b")
  }
  await waitFor(3)
  await settle()
} else if (scenario === "release") {
  await waitFor(1)
  await settle()
  navigate("ses_child")
  await waitFor(2)
  await settle()
} else if (scenario === "ignored") {
  // Starts on a child session (see HARNESS_ROUTE); neither it nor a session in
  // another directory may be reported.
  await settle()
  navigate("ses_elsewhere")
  await settle()
  navigate("ses_legacy")
  await waitFor(1)
  await settle()
  navigate("ses_a")
  await waitFor(2)
  await settle()
} else {
  throw new Error(`unknown scenario ${scenario}`)
}

teardown()
server.close()
process.stdout.write(`${JSON.stringify(received)}\n`)
process.exit(0)
