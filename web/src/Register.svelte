<script lang="ts">
  import { api, type Account, type Entry, type Register, type TxIn } from './api'

  let {
    accountId,
    accounts,
    onback,
    onjump,
    onchanged,
  }: {
    accountId: string
    accounts: Account[]
    onback: () => void
    onjump: (id: string) => void
    onchanged: () => void
  } = $props()

  let reg: Register | null = $state(null)
  let error: string | null = $state(null)
  let selected = $state(0)
  let expanded: Set<string> = $state(new Set())

  // ---- editor state ----
  interface EditSplit {
    account_id: string
    memo: string
    debit: string
    credit: string
    reconcile_state: string
  }
  interface Editor {
    txId: string | null // null = new transaction
    date: string
    num: string
    description: string
    splits: EditSplit[]
    focusField: number
  }
  let editor: Editor | null = $state(null)

  type Row =
    | { type: 'entry'; entry: Entry; i: number }
    | { type: 'split'; entry: Entry; splitIdx: number }

  let rows: Row[] = $derived.by(() => {
    const out: Row[] = []
    if (!reg) return out
    reg.entries.forEach((e, i) => {
      out.push({ type: 'entry', entry: e, i })
      if (expanded.has(e.tx_id)) {
        e.splits.forEach((_, si) => out.push({ type: 'split', entry: e, splitIdx: si }))
      }
    })
    return out
  })

  async function load(selectLast = true) {
    try {
      reg = await api.register(accountId)
      error = null
      if (selectLast) selected = Math.max(0, rows.length - 1)
      scrollSel()
    } catch (e: any) {
      error = e.message
    }
  }
  load()

  const acctById = $derived(new Map(accounts.map((a) => [a.id, a])))
  const selectableAccounts = $derived(
    [...accounts]
      .filter((a) => !a.placeholder)
      .sort((x, y) => (x.code ?? '￿').localeCompare(y.code ?? '￿') || x.name.localeCompare(y.name)),
  )

  function fmtSigned(amount: string): { debit: string; credit: string } {
    if (amount.startsWith('-')) return { debit: '', credit: amount.slice(1) }
    if (parseFloat(amount) === 0) return { debit: '', credit: '' }
    return { debit: amount, credit: '' }
  }

  function scrollSel() {
    requestAnimationFrame(() => {
      document.querySelector('.register-scroll tr.selected')?.scrollIntoView({ block: 'nearest' })
    })
  }

  // ---- date helpers for month/year jumps ----
  function shiftDate(iso: string, months: number, years: number): string {
    const [y, m, d] = iso.split('-').map(Number)
    const total = (y + years) * 12 + (m - 1) + months
    const ny = Math.floor(total / 12)
    const nm = (total % 12) + 1
    const maxDay = new Date(ny, nm, 0).getDate()
    return `${ny}-${String(nm).padStart(2, '0')}-${String(Math.min(d, maxDay)).padStart(2, '0')}`
  }

  function jumpToDate(target: string) {
    if (!reg) return
    let idx = rows.findIndex((r) => r.type === 'entry' && r.entry.date_posted >= target)
    if (idx < 0) idx = rows.length - 1
    selected = idx
    scrollSel()
  }

  function selectedEntry(): Entry | null {
    const r = rows[selected]
    return r ? r.entry : null
  }

  // ---- editor ----
  function todayISO(): string {
    const d = new Date()
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  }

  function openNew() {
    editor = {
      txId: null,
      date: reg?.entries.length ? reg.entries[reg.entries.length - 1].date_posted : todayISO(),
      num: '',
      description: '',
      splits: [
        { account_id: accountId, memo: '', debit: '', credit: '', reconcile_state: 'n' },
        { account_id: '', memo: '', debit: '', credit: '', reconcile_state: 'n' },
      ],
      focusField: 0,
    }
    focusEditor()
  }

  function openEdit() {
    const e = selectedEntry()
    if (!e) return
    editor = {
      txId: e.tx_id,
      date: e.date_posted,
      num: e.num ?? '',
      description: e.description ?? '',
      splits: e.splits.map((s) => {
        const dc = fmtSigned(s.value)
        return {
          account_id: s.account_id,
          memo: s.memo ?? '',
          debit: dc.debit,
          credit: dc.credit,
          reconcile_state: s.reconcile_state,
        }
      }),
      focusField: 0,
    }
    focusEditor()
  }

  function focusEditor() {
    requestAnimationFrame(() => {
      document.querySelector<HTMLInputElement>('.editor input[data-first]')?.focus()
    })
  }

  function addSplitLine() {
    if (!editor) return
    editor.splits.push({ account_id: '', memo: '', debit: '', credit: '', reconcile_state: 'n' })
  }

  function removeSplitLine(i: number) {
    if (!editor || editor.splits.length <= 2) return
    editor.splits.splice(i, 1)
  }

  function editorImbalance(): number {
    if (!editor) return 0
    let total = 0
    for (const s of editor.splits) {
      const d = parseFloat(s.debit || '0') || 0
      const c = parseFloat(s.credit || '0') || 0
      total += d - c
    }
    return Math.round(total * 100) / 100
  }

  async function saveEditor() {
    if (!editor || !reg) return
    try {
      const splits = editor.splits.filter((s) => s.account_id || s.debit || s.credit)
      // Auto-balance: if exactly one split has no amount, give it the remainder.
      const empty = splits.filter((s) => !s.debit && !s.credit)
      if (empty.length === 1) {
        const rem = -editorImbalance()
        if (rem > 0) empty[0].debit = rem.toFixed(2)
        else if (rem < 0) empty[0].credit = (-rem).toFixed(2)
      }
      const currency =
        reg.commodity_id?.startsWith('CURRENCY:') ? reg.commodity_id : 'CURRENCY:CAD'
      const tx: TxIn = {
        currency_id: currency,
        date_posted: editor.date,
        num: editor.num || undefined,
        description: editor.description || undefined,
        splits: splits.map((s) => ({
          account_id: s.account_id,
          memo: s.memo || undefined,
          reconcile_state: s.reconcile_state,
          value: s.debit ? s.debit : '-' + s.credit,
        })),
      }
      const keepDate = editor.date
      if (editor.txId) await api.updateTx(editor.txId, tx)
      else await api.createTx(tx)
      editor = null
      error = null
      await load(false)
      onchanged()
      jumpToDate(keepDate)
    } catch (e: any) {
      error = e.message
    }
  }

  async function deleteSelected() {
    const e = selectedEntry()
    if (!e) return
    if (!confirm(`Delete "${e.description ?? ''}" on ${e.date_posted}?`)) return
    try {
      await api.deleteTx(e.tx_id)
      await load(false)
      onchanged()
      selected = Math.min(selected, rows.length - 1)
    } catch (err: any) {
      error = err.message
    }
  }

  function jumpOther() {
    const r = rows[selected]
    if (!r) return
    if (r.type === 'split') {
      const s = r.entry.splits[r.splitIdx]
      if (s.account_id !== accountId) onjump(s.account_id)
      return
    }
    const others = r.entry.splits.filter((s) => s.account_id !== accountId)
    if (others.length >= 1) onjump(others[0].account_id)
  }

  function onkeydown(e: KeyboardEvent) {
    if (editor) {
      if (e.key === 'Escape') {
        editor = null
        e.preventDefault()
      } else if (e.key === 'Enter' && !e.shiftKey && !e.altKey) {
        saveEditor()
        e.preventDefault()
      } else if (e.key === 'Insert' || (e.altKey && e.key === 's')) {
        addSplitLine()
        e.preventDefault()
      }
      return
    }
    const n = rows.length
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
      case '[':
        jumpToDate(shiftDate(selectedEntry()?.date_posted ?? todayISO(), -1, 0))
        break
      case ']':
        jumpToDate(shiftDate(selectedEntry()?.date_posted ?? todayISO(), 1, 0))
        break
      case '{':
        jumpToDate(shiftDate(selectedEntry()?.date_posted ?? todayISO(), 0, -1))
        break
      case '}':
        jumpToDate(shiftDate(selectedEntry()?.date_posted ?? todayISO(), 0, 1))
        break
      case ' ': {
        const en = selectedEntry()
        if (en) {
          const next = new Set(expanded)
          if (next.has(en.tx_id)) next.delete(en.tx_id)
          else next.add(en.tx_id)
          expanded = next
        }
        break
      }
      case 'Enter':
      case 'e':
        openEdit()
        break
      case 'n':
        openNew()
        break
      case 'j':
        jumpOther()
        break
      case 'd':
      case 'Delete':
        deleteSelected()
        break
      case 'Escape':
      case 'Backspace':
        onback()
        break
      default:
        return
    }
    e.preventDefault()
    scrollSel()
  }

  const kindLabels = $derived.by(() => {
    const kind = acctById.get(accountId)?.kind ?? 'BANK'
    switch (kind) {
      case 'BANK':
      case 'ASSET':
        return { debit: 'Deposit', credit: 'Withdrawal' }
      case 'LIABILITY':
      case 'EQUITY':
        return { debit: 'Decrease', credit: 'Increase' }
      case 'INCOME':
        return { debit: 'Charge', credit: 'Income' }
      case 'EXPENSE':
        return { debit: 'Expense', credit: 'Rebate' }
      default:
        return { debit: 'Debit', credit: 'Credit' }
    }
  })

  function otherLabel(e: Entry): string {
    const others = e.splits.filter((s) => s.account_id !== accountId)
    if (others.length === 1) return others[0].account_name
    return '-- Split Transaction --'
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="scroll register-scroll">
  <table class="register">
    <thead>
      <tr>
        <th style="width: 7.5em">Date</th>
        <th style="width: 7em">Num</th>
        <th>Description</th>
        <th style="width: 18em">Transfer</th>
        <th style="width: 2em">R</th>
        <th class="amount" style="width: 8.5em">{kindLabels.debit}</th>
        <th class="amount" style="width: 8.5em">{kindLabels.credit}</th>
        <th class="balance" style="width: 9.5em">Balance</th>
      </tr>
    </thead>
    <tbody>
      {#each rows as row, i}
        {#if editor && row.type === 'entry' && editor.txId === row.entry.tx_id}
          <!-- editing this row: editor rendered below instead -->
        {/if}
        {#if row.type === 'entry'}
          {@const dc = fmtSigned(row.entry.amount)}
          {#if !(editor && editor.txId === row.entry.tx_id)}
            <tr
              class:selected={i === selected}
              class:alt={row.i % 2 === 1}
              onclick={() => (selected = i)}
              ondblclick={() => { selected = i; openEdit() }}
            >
              <td class="mono">{row.entry.date_posted}</td>
              <td class="mono">{row.entry.num ?? ''}</td>
              <td>{row.entry.description ?? ''}</td>
              <td>{otherLabel(row.entry)}</td>
              <td>{row.entry.reconcile_state}</td>
              <td class="amount mono">{dc.debit}</td>
              <td class="amount mono">{dc.credit}</td>
              <td class="balance mono" class:neg={row.entry.balance.startsWith('-')}>{row.entry.balance}</td>
            </tr>
          {/if}
        {:else}
          {@const s = row.entry.splits[row.splitIdx]}
          {@const dc = fmtSigned(s.value)}
          <tr
            class="splitrow"
            class:selected={i === selected}
            onclick={() => (selected = i)}
          >
            <td></td>
            <td></td>
            <td>{s.memo ?? ''}</td>
            <td>{s.account_name}</td>
            <td>{s.reconcile_state}</td>
            <td class="amount mono">{dc.debit}</td>
            <td class="amount mono">{dc.credit}</td>
            <td></td>
          </tr>
        {/if}
        {#if editor && editor.txId && row.type === 'entry' && editor.txId === row.entry.tx_id}
          {@render editorRows()}
        {/if}
      {/each}
      {#if editor && !editor.txId}
        {@render editorRows()}
      {/if}
    </tbody>
  </table>
</div>

{#snippet editorRows()}
  {#if editor}
    <tr class="editor">
      <td><input data-first type="date" bind:value={editor.date} /></td>
      <td><input type="text" bind:value={editor.num} placeholder="Num" /></td>
      <td colspan="5"><input type="text" bind:value={editor.description} placeholder="Description" /></td>
      <td class="balance mono" class:neg={editorImbalance() !== 0}>
        {editorImbalance() !== 0 ? `off ${editorImbalance().toFixed(2)}` : 'balanced'}
      </td>
    </tr>
    {#each editor.splits as s, si}
      <tr class="editor splitrow">
        <td></td>
        <td style="text-align:right; color: var(--dim)">{si + 1}</td>
        <td><input type="text" bind:value={s.memo} placeholder="Memo" /></td>
        <td>
          <select bind:value={s.account_id}>
            <option value="">— account —</option>
            {#each selectableAccounts as a (a.id)}
              <option value={a.id}>{a.code ? a.code + ' · ' : ''}{a.name}</option>
            {/each}
          </select>
        </td>
        <td>
          {#if editor.splits.length > 2}
            <button tabindex="-1" onclick={() => removeSplitLine(si)} title="Remove split">×</button>
          {/if}
        </td>
        <td><input type="text" bind:value={s.debit} placeholder={kindLabels.debit} style="text-align:right" /></td>
        <td><input type="text" bind:value={s.credit} placeholder={kindLabels.credit} style="text-align:right" /></td>
        <td></td>
      </tr>
    {/each}
    <tr class="editor">
      <td colspan="8" style="color: var(--dim)">
        Enter: save · Esc: cancel · Insert / Alt+S: add split line
      </td>
    </tr>
  {/if}
{/snippet}

{#if error}
  <div class="error-msg">{error}</div>
{/if}

<div class="statusbar">
  <span><b>{reg?.account_name ?? ''}</b></span>
  <span>{reg?.entries.length ?? 0} entries</span>
  {#if reg?.entries.length}
    <span>Balance: <b>{reg.entries[reg.entries.length - 1].balance}</b></span>
  {/if}
</div>
