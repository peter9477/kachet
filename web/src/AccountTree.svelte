<script lang="ts">
  import { api, type Account, type Commodity } from './api'
  import { ensureRowVisible } from './dom'

  let {
    accounts,
    onopen,
    onchanged,
  }: { accounts: Account[]; onopen: (id: string) => void; onchanged: () => void } = $props()

  interface Node {
    acct: Account
    depth: number
    hasChildren: boolean
  }

  let collapsed: Set<string> = $state(new Set())
  let selected = $state(0)
  let filter = $state('')
  let error: string | null = $state(null)

  // ---- account editor ----
  const KINDS = ['ASSET', 'BANK', 'CASH', 'LIABILITY', 'CREDIT', 'INCOME', 'EXPENSE', 'EQUITY', 'TRADING']
  interface Editor {
    id: string | null // null = new account
    name: string
    kind: string
    commodity_id: string
    parent_id: string
    code: string
    description: string
    placeholder: boolean
  }
  let editor: Editor | null = $state(null)
  let commodities: Commodity[] = $state([])

  async function ensureCommodities() {
    if (!commodities.length) {
      try {
        commodities = await api.commodities()
      } catch (e: any) {
        error = e.message
      }
    }
  }

  // Parent options: only accounts that exist in the visible book (root excluded).
  const parentOptions = $derived(
    [...accounts].sort(
      (x, y) => (x.code ?? '￿').localeCompare(y.code ?? '￿') || x.name.localeCompare(y.name),
    ),
  )

  function openNew() {
    const sel = rows[selected]?.acct
    ensureCommodities()
    editor = {
      id: null,
      name: '',
      kind: sel?.kind ?? 'ASSET',
      commodity_id: sel?.commodity_id ?? 'CURRENCY:CAD',
      // If sitting on a placeholder, default to nesting under it;
      // otherwise default to being a sibling of the selection.
      parent_id: sel ? (sel.placeholder ? sel.id : (sel.parent_id ?? '')) : '',
      code: '',
      description: '',
      placeholder: false,
    }
    focusEditor()
  }

  function openEdit() {
    const sel = rows[selected]?.acct
    if (!sel) return
    ensureCommodities()
    const ids = new Set(accounts.map((a) => a.id))
    editor = {
      id: sel.id,
      name: sel.name,
      kind: sel.kind,
      commodity_id: sel.commodity_id ?? 'CURRENCY:CAD',
      parent_id: sel.parent_id && ids.has(sel.parent_id) ? sel.parent_id : '',
      code: sel.code ?? '',
      description: sel.description ?? '',
      placeholder: sel.placeholder,
    }
    focusEditor()
  }

  function focusEditor() {
    requestAnimationFrame(() => {
      document.querySelector<HTMLInputElement>('.acct-editor input[data-first]')?.focus()
    })
  }

  async function saveEditor() {
    if (!editor) return
    try {
      const body = {
        name: editor.name,
        kind: editor.kind,
        commodity_id: editor.commodity_id,
        parent_id: editor.parent_id || null,
        code: editor.code || undefined,
        description: editor.description || undefined,
        placeholder: editor.placeholder,
      }
      const savedName = editor.name
      if (editor.id) await api.updateAccount(editor.id, body)
      else await api.createAccount(body)
      editor = null
      error = null
      onchanged()
      // Reselect the saved account once the refreshed list arrives.
      requestAnimationFrame(() => {
        const i = rows.findIndex((r) => r.acct.name === savedName)
        if (i >= 0) selected = i
      })
    } catch (e: any) {
      error = e.message
    }
  }

  async function deleteSelected() {
    const sel = rows[selected]?.acct
    if (!sel) return
    if (!confirm(`Delete account "${sel.name}"?`)) return
    try {
      await api.deleteAccount(sel.id)
      error = null
      onchanged()
    } catch (e: any) {
      error = e.message
    }
  }

  // Flatten the tree into visible rows, respecting collapse state.
  let rows: Node[] = $derived.by(() => {
    const byParent = new Map<string | null, Account[]>()
    const ids = new Set(accounts.map((a) => a.id))
    for (const a of accounts) {
      // Accounts whose parent is the (hidden) root show at top level
      const p = a.parent_id && ids.has(a.parent_id) ? a.parent_id : null
      if (!byParent.has(p)) byParent.set(p, [])
      byParent.get(p)!.push(a)
    }
    for (const list of byParent.values()) {
      list.sort((x, y) => (x.code ?? '￿').localeCompare(y.code ?? '￿') || x.name.localeCompare(y.name))
    }
    const out: Node[] = []
    const f = filter.toLowerCase()
    const matches = (a: Account) =>
      !f || a.name.toLowerCase().includes(f) || (a.code ?? '').includes(f)
    // With a filter active, show a flat list of matches instead of the tree.
    if (f) {
      for (const a of accounts.filter(matches)) {
        out.push({ acct: a, depth: 0, hasChildren: false })
      }
      return out
    }
    const walk = (parent: string | null, depth: number) => {
      for (const a of byParent.get(parent) ?? []) {
        const kids = byParent.has(a.id)
        out.push({ acct: a, depth, hasChildren: kids })
        if (kids && !collapsed.has(a.id)) walk(a.id, depth + 1)
      }
    }
    walk(null, 0)
    return out
  })

  $effect(() => {
    if (selected >= rows.length) selected = Math.max(0, rows.length - 1)
  })

  function onkeydown(e: KeyboardEvent) {
    if (editor) {
      if (e.key === 'Escape') {
        editor = null
        e.preventDefault()
      } else if (e.key === 'Enter') {
        saveEditor()
        e.preventDefault()
      }
      return
    }
    if (e.key === 'Insert') {
      openNew()
      e.preventDefault()
      return
    }
    if (e.key === 'F2') {
      openEdit()
      e.preventDefault()
      return
    }
    if (e.key === 'Delete') {
      deleteSelected()
      e.preventDefault()
      return
    }
    const n = rows.length
    if (!n) {
      if (e.key !== 'Escape' && e.key !== 'Backspace' && e.key.length === 1) filter += e.key
      return
    }
    switch (e.key) {
      case 'ArrowDown':
        selected = Math.min(n - 1, selected + 1)
        break
      case 'ArrowUp':
        selected = Math.max(0, selected - 1)
        break
      case 'PageDown':
        selected = Math.min(n - 1, selected + 20)
        break
      case 'PageUp':
        selected = Math.max(0, selected - 20)
        break
      case 'Home':
        selected = 0
        break
      case 'End':
        selected = n - 1
        break
      case 'ArrowLeft': {
        const node = rows[selected]
        if (node.hasChildren && !collapsed.has(node.acct.id)) {
          collapsed = new Set([...collapsed, node.acct.id])
        } else if (node.acct.parent_id) {
          const pi = rows.findIndex((r) => r.acct.id === node.acct.parent_id)
          if (pi >= 0) selected = pi
        }
        break
      }
      case 'ArrowRight': {
        const node = rows[selected]
        if (node.hasChildren && collapsed.has(node.acct.id)) {
          const next = new Set(collapsed)
          next.delete(node.acct.id)
          collapsed = next
        }
        break
      }
      case 'Enter':
        onopen(rows[selected].acct.id)
        break
      case 'Escape':
        filter = ''
        break
      case 'Backspace':
        filter = filter.slice(0, -1)
        break
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
          filter += e.key
          selected = 0
        } else {
          return
        }
    }
    e.preventDefault()
    scrollSelectedIntoView()
  }

  function scrollSelectedIntoView() {
    requestAnimationFrame(() => {
      const c = document.querySelector<HTMLElement>('.scroll')
      ensureRowVisible(c, c?.querySelector<HTMLElement>('tr.selected') ?? null)
    })
  }
</script>

<svelte:window onkeydown={onkeydown} />

{#if editor}
  <div class="acct-editor">
    <div class="acct-editor-grid">
      <label>Name <input data-first type="text" bind:value={editor.name} /></label>
      <label>Code <input type="text" bind:value={editor.code} /></label>
      <label>Type
        <select bind:value={editor.kind}>
          {#each KINDS as k}<option value={k}>{k}</option>{/each}
        </select>
      </label>
      <label>Parent
        <select bind:value={editor.parent_id}>
          <option value="">— top level —</option>
          {#each parentOptions as a (a.id)}
            {#if a.id !== editor.id}
              <option value={a.id}>{a.code ? a.code + ' · ' : ''}{a.name}</option>
            {/if}
          {/each}
        </select>
      </label>
      <label>Commodity
        <select bind:value={editor.commodity_id}>
          {#each commodities as c (c.id)}
            <option value={c.id}>{c.mnemonic} ({c.namespace})</option>
          {/each}
        </select>
      </label>
      <label>Description <input type="text" bind:value={editor.description} /></label>
      <label class="check"><input type="checkbox" bind:checked={editor.placeholder} /> Placeholder</label>
    </div>
    <div class="acct-editor-hint">
      {editor.id ? 'Editing account' : 'New account'} — Enter: save · Esc: cancel
    </div>
  </div>
{/if}

{#if error}
  <div class="error-msg">{error}</div>
{/if}

<div class="scroll">
  <table class="register">
    <thead>
      <tr>
        <th style="width: 6em">Code</th>
        <th>Account</th>
        <th style="width: 8em">Type</th>
        <th class="balance" style="width: 10em">Balance</th>
        <th style="width: 5em">Cmdty</th>
      </tr>
    </thead>
    <tbody>
      {#each rows as row, i (row.acct.id)}
        <tr
          class:selected={i === selected}
          class:alt={i % 2 === 1}
          onclick={() => (selected = i)}
          ondblclick={() => onopen(row.acct.id)}
        >
          <td class="mono">{row.acct.code ?? ''}</td>
          <td>
            <span class="tree-indent">{' '.repeat(row.depth * 3)}{row.hasChildren ? (collapsed.has(row.acct.id) ? '▸ ' : '▾ ') : ''}</span><span
              class:placeholder-name={row.acct.placeholder}>{row.acct.name}</span>
          </td>
          <td>{row.acct.kind}</td>
          <td class="balance mono" class:neg={row.acct.balance.startsWith('-')}>{row.acct.balance}</td>
          <td>{row.acct.commodity_id?.split(':')[1] ?? ''}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<div class="statusbar">
  <span><b>{rows.length}</b> accounts</span>
  {#if filter}<span>filter: <b>{filter}</b> (Esc clears)</span>{/if}
  <span>Insert: new · F2: edit · Del: delete</span>
</div>
