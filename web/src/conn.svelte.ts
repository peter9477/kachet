// Live backend connection status over a websocket. Reconnects with
// backoff; `conn.up` drives the header indicator. The server greets each
// connection with a hash of its web assets ("webhash"); if that changes
// across reconnects, newer frontend code is available and we surface a
// reload prompt rather than yanking the page out from under the user.

export const conn = $state({ up: false, updateAvailable: false })

let attempts = 0
// Hash of the assets this page was (presumably) loaded from: the first
// hello after page load. Later hellos that differ mean the server updated.
let loadedHash: string | null = null

function connect() {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  const ws = new WebSocket(`${proto}://${location.host}/api/ws`)

  ws.onopen = () => {
    conn.up = true
    attempts = 0
  }
  ws.onmessage = (ev) => {
    let msg: any
    try {
      msg = JSON.parse(ev.data)
    } catch {
      return
    }
    if (msg.type === 'hello' && typeof msg.web_hash === 'string') {
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
