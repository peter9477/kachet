// REST client. Shapes mirror the Rust structs in src/api.rs.
//
// @typedef {{id: string, name: string, kind: string, commodity_id: string?,
//   parent_id: string?, code: string?, description: string?, placeholder: boolean,
//   hidden: boolean, balance: string, tx_count: number}} Account
// @typedef {{id: string, account_id: string, account_name: string, memo: string?,
//   reconcile_state: string, value: string, quantity: string}} Split
// @typedef {{tx_id: string, date_posted: string, num: string?, description: string?,
//   currency_id: string, split_id: string, memo: string?, reconcile_state: string,
//   amount: string, balance: string, splits: Split[]}} Entry

async function req(url, init) {
  const r = await fetch(url, init)
  if (!r.ok) {
    let msg = r.statusText
    try {
      msg = (await r.json()).error ?? msg
    } catch {}
    throw new Error(msg)
  }
  return r.json()
}

const json = (method, body) => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
})

export const api = {
  accounts: () => req('/api/accounts'),
  commodities: () => req('/api/commodities'),
  register: (id) => req(`/api/accounts/${id}/register`),
  createAccount: (a) => req('/api/accounts', json('POST', a)),
  updateAccount: (id, a) => req(`/api/accounts/${id}`, json('PUT', a)),
  deleteAccount: (id) => req(`/api/accounts/${id}`, { method: 'DELETE' }),
  createTx: (tx) => req('/api/transactions', json('POST', tx)),
  updateTx: (id, tx) => req(`/api/transactions/${id}`, json('PUT', tx)),
  deleteTx: (id) => req(`/api/transactions/${id}`, { method: 'DELETE' }),
}
