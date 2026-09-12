// OpenCode >=2.0.2 CLI integration. The CLI owns the selected pane/session;
// the shared OpenCode service must never report ownership for another TUI.
import net from "node:net"
import { createEffect } from "solid-js"

const MAX_REPLY_BYTES = 64 * 1024
const REQUEST_TIMEOUT_MS = 500
// Bounded backoff: a transient socket or timeout failure is retried a few
// times and then dropped, so a permanently unreachable server cannot keep a
// request alive forever.
const RETRY_DELAYS_MS = [100, 400, 1600]
const REPORT = "pane.report_session"
const RELEASE = "pane.release_session"

function samePath(left, right) {
  if (typeof left !== "string" || typeof right !== "string") return false
  const clean = (value) => {
    const path = value.replaceAll("\\", "/").replace(/\/+$/, "")
    return process.platform === "win32" ? path.toLowerCase() : path
  }
  return clean(left) === clean(right)
}

export function reportFor(context) {
  const route = context.ui.router.current()
  if (route?.type !== "session" || !route.sessionID) return
  const info = context.data.session.get(route.sessionID)
  if (!info || info.id !== route.sessionID || info.parentID) return
  const location = context.location ?? context.data.location.default()
  // 2.0.3 nests the session's own directory under `location`; 2.0.2 kept it flat.
  const directory = info.location?.directory ?? info.directory
  if (!samePath(directory, location?.directory)) return
  return { pane: process.env.LUVUS_PANE_ID, agent: "opencode", session_id: info.id }
}

function sendRequest(address, id, method, params, onSocket) {
  return new Promise((resolve) => {
    let finished = false
    let bytes = 0
    let response = ""
    const socket = net.createConnection(address)
    onSocket?.(socket)
    const finish = (ok) => {
      if (finished) return
      finished = true
      clearTimeout(timeout)
      socket.destroy()
      onSocket?.(undefined)
      resolve(ok)
    }
    const timeout = setTimeout(() => finish(false), REQUEST_TIMEOUT_MS)
    socket.on("connect", () => socket.write(`${JSON.stringify({ id, method, params })}\n`))
    socket.on("data", (chunk) => {
      bytes += chunk.length
      if (bytes > MAX_REPLY_BYTES) return finish(false)
      response += chunk.toString("utf8")
      const newline = response.indexOf("\n")
      if (newline < 0) return
      try {
        const reply = JSON.parse(response.slice(0, newline))
        finish(reply.id === id && reply.result?.type === "ok")
      } catch { finish(false) }
    })
    socket.on("error", () => finish(false))
    socket.on("end", () => finish(false))
  })
}

export default {
  id: "luvus.session.v2",
  setup(context) {
    const address = process.env.LUVUS_API_ADDRESS || process.env.LUVUS_SOCKET_PATH
    if (process.env.LUVUS_ENV !== "1" || !address || !process.env.LUVUS_PANE_ID) return
    let disposed = false
    let pending // the newest unsent request; it always wins over an older retry
    let socket
    let sending = false
    let sequence = 0
    let settled = "" // fingerprint of the last request the server accepted
    let owned // exact identity this pane currently owns, for a fenced release
    let wake
    const sleep = (ms) =>
      new Promise((resolve) => {
        const timer = setTimeout(() => wake?.(), ms)
        wake = () => {
          clearTimeout(timer)
          wake = undefined
          resolve()
        }
      })
    const drain = async () => {
      if (sending) return
      sending = true
      let attempt = 0
      while (!disposed && pending) {
        const request = pending
        const id = `luvus-v2-${++sequence}`
        const ok = await sendRequest(address, id, request.method, request.params, (value) => {
          socket = value
        })
        // A newer report arrived while this one was in flight: retry state
        // belongs to the request that produced it, so start the new one clean.
        if (pending !== request) {
          attempt = 0
          continue
        }
        if (ok) {
          pending = undefined
          settled = request.fingerprint
          owned = request.method === REPORT ? request.params : undefined
          attempt = 0
          continue
        }
        // Keep the failed request and retry it; give up once backoff runs out.
        const delay = RETRY_DELAYS_MS[attempt]
        if (delay === undefined) {
          pending = undefined
          attempt = 0
          continue
        }
        attempt += 1
        await sleep(delay)
      }
      sending = false
    }
    const enqueue = (method, params) => {
      const fingerprint = `${method} ${JSON.stringify(params)}`
      if (fingerprint === settled || fingerprint === pending?.fingerprint) return
      pending = { method, params, fingerprint }
      wake?.()
      void drain()
    }
    const publish = () => {
      if (disposed) return
      const params = reportFor(context)
      // Leaving the reported root session hands the exact binding back; the
      // server ignores it if a newer session already replaced ours.
      if (params) enqueue(REPORT, params)
      else if (owned) enqueue(RELEASE, owned)
    }
    // Reactive route/session reads handle navigation without an idle poll.
    const removeSlot = context.ui.slot({
      append: "app",
      render: () => { createEffect(publish); return null },
    })
    // A cache refresh can also make an initially cold selected session ready.
    const stop = context.data.on("session.updated", publish)
    return () => {
      disposed = true
      pending = undefined
      wake?.()
      socket?.destroy()
      stop()
      removeSlot()
    }
  },
}
