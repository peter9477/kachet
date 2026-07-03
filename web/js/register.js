import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue'
import { api } from './api.js'
import { ensureRowVisible, fmtSigned } from './dom.js'

function todayISO() {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

// Shift an ISO date by whole months/years, clamping the day-of-month.
function shiftDate(iso, months, years) {
  const [y, m, d] = iso.split('-').map(Number)
  const total = (y + years) * 12 + (m - 1) + months
  const ny = Math.floor(total / 12)
  const nm = (total % 12) + 1
  const maxDay = new Date(ny, nm, 0).getDate()
  return `${ny}-${String(nm).padStart(2, '0')}-${String(Math.min(d, maxDay)).padStart(2, '0')}`
}

export default {
  name: 'Register',
  props: {
    accountId: { type: String, required: true },
    accounts: { type: Array, required: true },
  },
  emits: ['back', 'jump', 'changed'],
  setup(props, { emit }) {
    const reg = ref(null)
    const error = ref(null)
    const selected = ref(0)
    const expanded = ref(new Set())
    // editor: {txId: string?, date, num, description,
    //          splits: [{account_id, memo, debit, credit, reconcile_state}]}
    const editor = ref(null)

    const rows = computed(() => {
      const out = []
      if (!reg.value) return out
      reg.value.entries.forEach((e, i) => {
        out.push({ type: 'entry', entry: e, i })
        if (expanded.value.has(e.tx_id)) {
          e.splits.forEach((_, si) => out.push({ type: 'split', entry: e, splitIdx: si }))
        }
      })
      return out
    })

    async function load(selectLast = true) {
      try {
        reg.value = await api.register(props.accountId)
        error.value = null
        if (selectLast) selected.value = Math.max(0, rows.value.length - 1)
        scrollSel()
      } catch (e) {
        error.value = e.message
      }
    }
    load()

    const acct = computed(() => props.accounts.find((a) => a.id === props.accountId))
    const selectableAccounts = computed(() =>
      props.accounts
        .filter((a) => !a.placeholder)
        .sort((x, y) => (x.code ?? '￿').localeCompare(y.code ?? '￿') || x.name.localeCompare(y.name)),
    )

    const kindLabels = computed(() => {
      switch (acct.value?.kind ?? 'BANK') {
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

    function otherLabel(e) {
      const others = e.splits.filter((s) => s.account_id !== props.accountId)
      if (others.length === 1) return others[0].account_name
      return '-- Split Transaction --'
    }

    function scrollSel() {
      nextTick(() => {
        const c = document.querySelector('.register-scroll')
        ensureRowVisible(c, c?.querySelector('tr.selected'))
      })
    }

    function jumpToDate(target) {
      if (!reg.value) return
      let idx = rows.value.findIndex((r) => r.type === 'entry' && r.entry.date_posted >= target)
      if (idx < 0) idx = rows.value.length - 1
      selected.value = idx
      scrollSel()
    }

    const selectedEntry = () => rows.value[selected.value]?.entry ?? null

    // ---- editor ----
    function focusEditor() {
      nextTick(() => document.querySelector('.editor input[data-first]')?.focus())
    }

    function openNew() {
      editor.value = {
        txId: null,
        date: reg.value?.entries.length
          ? reg.value.entries[reg.value.entries.length - 1].date_posted
          : todayISO(),
        num: '',
        description: '',
        splits: [
          { account_id: props.accountId, memo: '', debit: '', credit: '', reconcile_state: 'n' },
          { account_id: '', memo: '', debit: '', credit: '', reconcile_state: 'n' },
        ],
      }
      focusEditor()
    }

    function openEdit() {
      const e = selectedEntry()
      if (!e) return
      editor.value = {
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
      }
      focusEditor()
    }

    function addSplitLine() {
      editor.value?.splits.push({ account_id: '', memo: '', debit: '', credit: '', reconcile_state: 'n' })
    }

    function removeSplitLine(i) {
      const ed = editor.value
      if (!ed || ed.splits.length <= 2) return
      ed.splits.splice(i, 1)
    }

    function editorImbalance() {
      const ed = editor.value
      if (!ed) return 0
      let total = 0
      for (const s of ed.splits) {
        total += (parseFloat(s.debit || '0') || 0) - (parseFloat(s.credit || '0') || 0)
      }
      return Math.round(total * 100) / 100
    }

    async function saveEditor() {
      const ed = editor.value
      if (!ed || !reg.value) return
      try {
        const splits = ed.splits.filter((s) => s.account_id || s.debit || s.credit)
        // Auto-balance: if exactly one split has no amount, give it the remainder.
        const empty = splits.filter((s) => !s.debit && !s.credit)
        if (empty.length === 1) {
          const rem = -editorImbalance()
          if (rem > 0) empty[0].debit = rem.toFixed(2)
          else if (rem < 0) empty[0].credit = (-rem).toFixed(2)
        }
        const currency = reg.value.commodity_id?.startsWith('CURRENCY:')
          ? reg.value.commodity_id
          : 'CURRENCY:CAD'
        const tx = {
          currency_id: currency,
          date_posted: ed.date,
          num: ed.num || undefined,
          description: ed.description || undefined,
          splits: splits.map((s) => ({
            account_id: s.account_id,
            memo: s.memo || undefined,
            reconcile_state: s.reconcile_state,
            value: s.debit ? s.debit : '-' + s.credit,
          })),
        }
        const keepDate = ed.date
        if (ed.txId) await api.updateTx(ed.txId, tx)
        else await api.createTx(tx)
        editor.value = null
        error.value = null
        await load(false)
        emit('changed')
        jumpToDate(keepDate)
      } catch (e) {
        error.value = e.message
      }
    }

    async function deleteSelected() {
      const e = selectedEntry()
      if (!e) return
      if (!confirm(`Delete "${e.description ?? ''}" on ${e.date_posted}?`)) return
      try {
        await api.deleteTx(e.tx_id)
        await load(false)
        emit('changed')
        selected.value = Math.min(selected.value, rows.value.length - 1)
      } catch (err) {
        error.value = err.message
      }
    }

    function jumpOther() {
      const r = rows.value[selected.value]
      if (!r) return
      if (r.type === 'split') {
        const s = r.entry.splits[r.splitIdx]
        if (s.account_id !== props.accountId) emit('jump', s.account_id)
        return
      }
      const others = r.entry.splits.filter((s) => s.account_id !== props.accountId)
      if (others.length >= 1) emit('jump', others[0].account_id)
    }

    function toggleSplits() {
      const en = selectedEntry()
      if (!en) return
      const next = new Set(expanded.value)
      const opening = !next.has(en.tx_id)
      if (opening) next.add(en.tx_id)
      else next.delete(en.tx_id)
      expanded.value = next
      if (!opening) {
        // Collapsing while on a split row: keep the selection on this
        // transaction by moving it to the parent entry row.
        const idx = rows.value.findIndex((r) => r.type === 'entry' && r.entry === en)
        if (idx >= 0) selected.value = idx
      } else {
        // Bring the newly revealed split rows onscreen, but never at
        // the cost of scrolling the selected entry itself out of view.
        nextTick(() => {
          const c = document.querySelector('.register-scroll')
          const splits = c?.querySelectorAll(`tr[data-tx="${en.tx_id}"]`)
          if (splits?.length) ensureRowVisible(c, splits[splits.length - 1])
          ensureRowVisible(c, c?.querySelector('tr.selected'))
        })
      }
    }

    function onkeydown(e) {
      if (editor.value) {
        if (e.key === 'Escape') {
          editor.value = null
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
      const n = rows.value.length
      switch (e.key) {
        case 'ArrowDown':
          selected.value = Math.min(n - 1, selected.value + 1)
          break
        case 'ArrowUp':
          selected.value = Math.max(0, selected.value - 1)
          break
        case 'PageDown':
          selected.value = Math.min(n - 1, selected.value + 20)
          break
        case 'PageUp':
          selected.value = Math.max(0, selected.value - 20)
          break
        case 'Home':
          selected.value = 0
          break
        case 'End':
          selected.value = n - 1
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
        case ' ':
          toggleSplits()
          break
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
          emit('back')
          break
        default:
          return
      }
      e.preventDefault()
      scrollSel()
    }

    onMounted(() => window.addEventListener('keydown', onkeydown))
    onUnmounted(() => window.removeEventListener('keydown', onkeydown))

    return {
      reg, error, selected, expanded, editor, rows, kindLabels, selectableAccounts,
      fmtSigned, otherLabel, openEdit, removeSplitLine, editorImbalance,
      select: (i) => (selected.value = i),
    }
  },
  template: `
<div class="scroll register-scroll">
  <table class="register">
    <thead>
      <tr>
        <th style="width: 7.5em">Date</th>
        <th style="width: 7em">Num</th>
        <th>Description</th>
        <th style="width: 18em">Transfer</th>
        <th style="width: 2em">R</th>
        <th class="amount" style="width: 8.5em">{{ kindLabels.debit }}</th>
        <th class="amount" style="width: 8.5em">{{ kindLabels.credit }}</th>
        <th class="balance" style="width: 9.5em">Balance</th>
      </tr>
    </thead>
    <tbody>
      <template v-for="(row, i) in rows">
        <tr v-if="row.type === 'entry' && !(editor && editor.txId === row.entry.tx_id)"
            :key="row.entry.split_id"
            :class="{selected: i === selected, alt: row.i % 2 === 1}"
            @click="select(i)" @dblclick="select(i); openEdit()">
          <td class="mono">{{ row.entry.date_posted }}</td>
          <td class="mono">{{ row.entry.num ?? '' }}</td>
          <td>{{ row.entry.description ?? '' }}</td>
          <td>{{ expanded.has(row.entry.tx_id) ? '▾ ' : '' }}{{ otherLabel(row.entry) }}</td>
          <td>{{ row.entry.reconcile_state }}</td>
          <td class="amount mono">{{ fmtSigned(row.entry.amount).debit }}</td>
          <td class="amount mono">{{ fmtSigned(row.entry.amount).credit }}</td>
          <td class="balance mono" :class="{neg: row.entry.balance.startsWith('-')}">{{ row.entry.balance }}</td>
        </tr>
        <tr v-else-if="row.type === 'split'"
            :key="row.entry.tx_id + ':' + row.splitIdx"
            class="splitrow" :class="{selected: i === selected}"
            :data-tx="row.entry.tx_id" @click="select(i)">
          <td></td>
          <td></td>
          <td>{{ row.entry.splits[row.splitIdx].memo ?? '' }}</td>
          <td>{{ row.entry.splits[row.splitIdx].account_name }}</td>
          <td>{{ row.entry.splits[row.splitIdx].reconcile_state }}</td>
          <td class="amount mono">{{ fmtSigned(row.entry.splits[row.splitIdx].value).debit }}</td>
          <td class="amount mono">{{ fmtSigned(row.entry.splits[row.splitIdx].value).credit }}</td>
          <td></td>
        </tr>
        <template v-if="editor && editor.txId && row.type === 'entry' && editor.txId === row.entry.tx_id">
          <tr class="editor">
            <td><input data-first type="date" v-model="editor.date" /></td>
            <td><input type="text" v-model="editor.num" placeholder="Num" /></td>
            <td colspan="5"><input type="text" v-model="editor.description" placeholder="Description" /></td>
            <td class="balance mono" :class="{neg: editorImbalance() !== 0}">
              {{ editorImbalance() !== 0 ? 'off ' + editorImbalance().toFixed(2) : 'balanced' }}
            </td>
          </tr>
          <tr v-for="(s, si) in editor.splits" :key="si" class="editor splitrow">
            <td></td>
            <td style="text-align:right; color: var(--dim)">{{ si + 1 }}</td>
            <td><input type="text" v-model="s.memo" placeholder="Memo" /></td>
            <td>
              <select v-model="s.account_id">
                <option value="">— account —</option>
                <option v-for="a in selectableAccounts" :key="a.id" :value="a.id">{{ a.code ? a.code + ' · ' : '' }}{{ a.name }}</option>
              </select>
            </td>
            <td>
              <button v-if="editor.splits.length > 2" tabindex="-1" @click="removeSplitLine(si)" title="Remove split">×</button>
            </td>
            <td><input type="text" v-model="s.debit" :placeholder="kindLabels.debit" style="text-align:right" /></td>
            <td><input type="text" v-model="s.credit" :placeholder="kindLabels.credit" style="text-align:right" /></td>
            <td></td>
          </tr>
          <tr class="editor">
            <td colspan="8" style="color: var(--dim)">
              Enter: save · Esc: cancel · Insert / Alt+S: add split line
            </td>
          </tr>
        </template>
      </template>
      <template v-if="editor && !editor.txId">
        <tr class="editor">
          <td><input data-first type="date" v-model="editor.date" /></td>
          <td><input type="text" v-model="editor.num" placeholder="Num" /></td>
          <td colspan="5"><input type="text" v-model="editor.description" placeholder="Description" /></td>
          <td class="balance mono" :class="{neg: editorImbalance() !== 0}">
            {{ editorImbalance() !== 0 ? 'off ' + editorImbalance().toFixed(2) : 'balanced' }}
          </td>
        </tr>
        <tr v-for="(s, si) in editor.splits" :key="si" class="editor splitrow">
          <td></td>
          <td style="text-align:right; color: var(--dim)">{{ si + 1 }}</td>
          <td><input type="text" v-model="s.memo" placeholder="Memo" /></td>
          <td>
            <select v-model="s.account_id">
              <option value="">— account —</option>
              <option v-for="a in selectableAccounts" :key="a.id" :value="a.id">{{ a.code ? a.code + ' · ' : '' }}{{ a.name }}</option>
            </select>
          </td>
          <td>
            <button v-if="editor.splits.length > 2" tabindex="-1" @click="removeSplitLine(si)" title="Remove split">×</button>
          </td>
          <td><input type="text" v-model="s.debit" :placeholder="kindLabels.debit" style="text-align:right" /></td>
          <td><input type="text" v-model="s.credit" :placeholder="kindLabels.credit" style="text-align:right" /></td>
          <td></td>
        </tr>
        <tr class="editor">
          <td colspan="8" style="color: var(--dim)">
            Enter: save · Esc: cancel · Insert / Alt+S: add split line
          </td>
        </tr>
      </template>
    </tbody>
  </table>
</div>

<div v-if="error" class="error-msg">{{ error }}</div>

<div class="statusbar">
  <span><b>{{ reg?.account_name ?? '' }}</b></span>
  <span>{{ reg?.entries.length ?? 0 }} entries</span>
  <span v-if="reg?.entries.length">Balance: <b>{{ reg.entries[reg.entries.length - 1].balance }}</b></span>
</div>
`,
}
