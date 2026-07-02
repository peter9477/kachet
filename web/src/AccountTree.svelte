<script lang="ts">
  import type { Account } from './api'

  let { accounts, onopen }: { accounts: Account[]; onopen: (id: string) => void } = $props()

  interface Node {
    acct: Account
    depth: number
    hasChildren: boolean
  }

  let collapsed: Set<string> = $state(new Set())
  let selected = $state(0)
  let filter = $state('')

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
    const n = rows.length
    if (!n) return
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
      document.querySelector('tr.selected')?.scrollIntoView({ block: 'nearest' })
    })
  }
</script>

<svelte:window onkeydown={onkeydown} />

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
</div>
