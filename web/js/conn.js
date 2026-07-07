// Live backend connection status over a websocket. Reconnects with
// backoff; `conn.up` drives the header indicator. The server sends a hash
// of its web assets ("webhash") on connect and every ~5s after; if it
// ever differs from the hash at page load, newer frontend code is
// available and we surface a reload prompt rather than yanking the page
// out from under the user.

import { reactive } from 'vue'

export const conn = reactive({ up: false, updateAvailable: false, version: '' })

let attempts = 0
// Hash of the assets this page was (presumably) loaded from: the first
// hello after page load. Later hellos that differ mean the server updated.
let loadedHash = null

function connect() {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  const ws = new WebSocket(`${proto}://${location.host}/api/ws`)

  ws.onopen = () => {
    conn.up = true
    attempts = 0
  }
  ws.onmessage = (ev) => {
    let msg
    try {
      msg = JSON.parse(ev.data)
    } catch {
      return
    }
    if (msg.type === 'hello' && typeof msg.web_hash === 'string') {
      if (typeof msg.version === 'string') conn.version = msg.version
      if (loadedHash === null) loadedHash = msg.web_hash
      else if (msg.web_hash !== loadedHash) conn.updateAvailable = true
    }
  }
  ws.onclose = () => {
    conn.up = false
    const delay = Math.min(5000, 250 * 2 ** attempts++)
    setTimeout(connect, delay)
  }
  ws.onerror = () => {
    ws.close()
  }
}

connect()
