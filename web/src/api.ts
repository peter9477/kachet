export interface Account {
  id: string
  name: string
  kind: string
  commodity_id: string | null
  parent_id: string | null
  code: string | null
  description: string | null
  placeholder: boolean
  hidden: boolean
  balance: string
  tx_count: number
}

export interface Commodity {
  id: string
  namespace: string
  mnemonic: string
  fraction: number
}

export interface AccountIn {
  name: string
  kind: string
  commodity_id: string
  parent_id?: string | null
  code?: string
  description?: string
  placeholder?: boolean
  hidden?: boolean
}

export interface Split {
  id: string
  account_id: string
  account_name: string
  memo: string | null
  reconcile_state: string
  value: string
  quantity: string
}

export interface Entry {
  tx_id: string
  date_posted: string
  num: string | null
  description: string | null
  currency_id: string
  split_id: string
  memo: string | null
  reconcile_state: string
  amount: string
  balance: string
  splits: Split[]
}

export interface Register {
  account_id: string
  account_name: string
  commodity_id: string | null
  entries: Entry[]
}

export interface SplitIn {
  account_id: string
  memo?: string
  value: string
  quantity?: string
  reconcile_state?: string
}

export interface TxIn {
  currency_id: string
  date_posted: string
  num?: string
  description?: string
  splits: SplitIn[]
}

async function req<T>(url: string, init?: RequestInit): Promise<T> {
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

export const api = {
  accounts: () => req<Account[]>('/api/accounts'),
  commodities: () => req<Commodity[]>('/api/commodities'),
  createAccount: (a: AccountIn) =>
    req<{ id: string }>('/api/accounts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(a),
    }),
  updateAccount: (id: string, a: AccountIn) =>
    req<{ id: string }>(`/api/accounts/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(a),
    }),
  deleteAccount: (id: string) =>
    req<{ deleted: string }>(`/api/accounts/${id}`, { method: 'DELETE' }),
  register: (id: string) => req<Register>(`/api/accounts/${id}/register`),
  createTx: (tx: TxIn) =>
    req<{ id: string }>('/api/transactions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(tx),
    }),
  updateTx: (id: string, tx: TxIn) =>
    req<{ id: string }>(`/api/transactions/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(tx),
    }),
  deleteTx: (id: string) =>
    req<{ deleted: string }>(`/api/transactions/${id}`, { method: 'DELETE' }),
}
