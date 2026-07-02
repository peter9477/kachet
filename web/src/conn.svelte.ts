// Live backend connection status over a websocket. Reconnects with
// backoff; `conn.up` drives the header indicator. Server-push messages
// (JSON text frames) will be dispatched from here in future.

export const conn = $state({ up: false })

let attempts = 0

function connect() {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  const ws = new WebSocket(`${proto}://${location.host}/api/ws`)

  ws.onopen = () => {
    conn.up = true
    attempts = 0
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
