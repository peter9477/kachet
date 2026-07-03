import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { api } from './api.js'
import { ensureRowVisible } from './dom.js'
import { hints } from './keys.js'

const KINDS = ['ASSET', 'BANK', 'CASH', 'LIABILITY', 'CREDIT', 'INCOME', 'EXPENSE', 'EQUITY', 'TRADING']

export default {
  name: 'AccountTree',
  props: { accounts: { type: Array, required: true } },
  emits: ['open', 'changed'],
  setup(props, { emit }) {
    const collapsed = ref(new Set())
    const selected = ref(0)
    const filter = ref('')
    const error = ref(null)
    const editor = ref(null) // {id: string?, name, kind, commodity_id, parent_id, code, description, placeholder}
    const commodities = ref([])

    // Flatten the tree into visible rows, respecting collapse state.
    // With a filter active, show a flat list of matches instead.
    const rows = computed(() => {
      const byParent = new Map()
      const ids = new Set(props.accounts.map((a) => a.id))
      for (const a of props.accounts) {
        // Accounts whose parent is the (hidden) root show at top level
        const p = a.parent_id && ids.has(a.parent_id) ? a.parent_id : null
        if (!byParent.has(p)) byParent.set(p, [])
        byParent.get(p).push(a)
      }
      for (const list of byParent.values()) {
        list.sort((x, y) => (x.code ?? '￿').localeCompare(y.code ?? '￿') || x.name.localeCompare(y.name))
      }
      const out = []
      const f = filter.value.toLowerCase()
      if (f) {
        for (const a of props.accounts) {
          if (a.name.toLowerCase().includes(f) || (a.code ?? '').includes(f)) {
            out.push({ acct: a, depth: 0, hasChildren: false })
          }
        }
        return out
      }
      const walk = (parent, depth) => {
        for (const a of byParent.get(parent) ?? []) {
          const kids = byParent.has(a.id)
          out.push({ acct: a, depth, hasChildren: kids })
          if (kids && !collapsed.value.has(a.id)) walk(a.id, depth + 1)
        }
      }
      walk(null, 0)
      return out
    })

    watch(rows, () => {
      if (selected.value >= rows.value.length) selected.value = Math.max(0, rows.value.length - 1)
    })

    const parentOptions = computed(() =>
      [...props.accounts].sort(
        (x, y) => (x.code ?? '￿').localeCompare(y.code ?? '￿') || x.name.localeCompare(y.name),
      ),
    )

    async function ensureCommodities() {
      if (!commodities.value.length) {
        try {
          commodities.value = await api.commodities()
        } catch (e) {
          error.value = e.message
        }
      }
    }

    function focusEditor() {
      nextTick(() => document.querySelector('.acct-editor input[data-first]')?.focus())
    }

    function openNew() {
      const sel = rows.value[selected.value]?.acct
      ensureCommodities()
      editor.value = {
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
      const sel = rows.value[selected.value]?.acct
      if (!sel) return
      ensureCommodities()
      const ids = new Set(props.accounts.map((a) => a.id))
      editor.value = {
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

    async function saveEditor() {
      const ed = editor.value
      if (!ed) return
      try {
        const body = {
          name: ed.name,
          kind: ed.kind,
          commodity_id: ed.commodity_id,
          parent_id: ed.parent_id || null,
          code: ed.code || undefined,
          description: ed.description || undefined,
          placeholder: ed.placeholder,
        }
        const savedName = ed.name
        if (ed.id) await api.updateAccount(ed.id, body)
        else await api.createAccount(body)
        editor.value = null
        error.value = null
        emit('changed')
        // Reselect the saved account once the refreshed list arrives.
        nextTick(() => {
          const i = rows.value.findIndex((r) => r.acct.name === savedName)
          if (i >= 0) selected.value = i
        })
      } catch (e) {
        error.value = e.message
      }
    }

    async function deleteSelected() {
      const sel = rows.value[selected.value]?.acct
      if (!sel) return
      if (!confirm(`Delete account "${sel.name}"?`)) return
      try {
        await api.deleteAccount(sel.id)
        error.value = null
        emit('changed')
      } catch (e) {
        error.value = e.message
      }
    }

    function scrollSel() {
      nextTick(() => {
        const c = document.querySelector('.scroll')
        ensureRowVisible(c, c?.querySelector('tr.selected'))
      })
    }

    function onkeydown(e) {
      if (editor.value) {
        if (e.key === 'Escape') {
          editor.value = null
          e.preventDefault()
        } else if (e.key === 'Enter') {
          saveEditor()
          e.preventDefault()
        }
        return
      }
      // Mac keyboards have no Insert, and their Delete is Backspace
      // (used here for filter editing) — so each action has a
      // modifier-based alternate. Ctrl+letter is safe on macOS because
      // the browser's own shortcuts live on Cmd.
      if (e.key === 'Insert' || (e.ctrlKey && e.code === 'KeyN')) {
        openNew()
        e.preventDefault()
        return
      }
      if (e.key === 'F2' || (e.ctrlKey && e.code === 'KeyE')) {
        openEdit()
        e.preventDefault()
        return
      }
      if (e.key === 'Delete' || ((e.metaKey || e.ctrlKey) && e.key === 'Backspace')) {
        deleteSelected()
        e.preventDefault()
        return
      }
      const n = rows.value.length
      if (!n) {
        if (e.key !== 'Escape' && e.key !== 'Backspace' && e.key.length === 1) filter.value += e.key
        return
      }
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
        case 'ArrowLeft': {
          const node = rows.value[selected.value]
          if (node.hasChildren && !collapsed.value.has(node.acct.id)) {
            collapsed.value = new Set([...collapsed.value, node.acct.id])
          } else if (node.acct.parent_id) {
            const pi = rows.value.findIndex((r) => r.acct.id === node.acct.parent_id)
            if (pi >= 0) selected.value = pi
          }
          break
        }
        case 'ArrowRight': {
          const node = rows.value[selected.value]
          if (node.hasChildren && collapsed.value.has(node.acct.id)) {
            const next = new Set(collapsed.value)
            next.delete(node.acct.id)
            collapsed.value = next
          }
          break
        }
        case 'Enter':
          emit('open', rows.value[selected.value].acct.id)
          break
        case 'Escape':
          filter.value = ''
          break
        case 'Backspace':
          filter.value = filter.value.slice(0, -1)
          break
        default:
          if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
            filter.value += e.key
            selected.value = 0
          } else {
            return
          }
      }
      e.preventDefault()
      scrollSel()
    }

    onMounted(() => window.addEventListener('keydown', onkeydown))
    onUnmounted(() => window.removeEventListener('keydown', onkeydown))

    const indent = (depth) => ' '.repeat(depth * 3)

    return {
      KINDS, hints, collapsed, selected, filter, error, editor, commodities, rows, parentOptions,
      openNew, openEdit, saveEditor, deleteSelected, indent,
      open: (id) => emit('open', id),
    }
  },
  template: `
<div v-if="editor" class="acct-editor">
  <div class="acct-editor-grid">
    <label>Name <input data-first type="text" v-model="editor.name" /></label>
    <label>Code <input type="text" v-model="editor.code" /></label>
    <label>Type
      <select v-model="editor.kind">
        <option v-for="k in KINDS" :key="k" :value="k">{{ k }}</option>
      </select>
    </label>
    <label>Parent
      <select v-model="editor.parent_id">
        <option value="">— top level —</option>
        <template v-for="a in parentOptions" :key="a.id">
          <option v-if="a.id !== editor.id" :value="a.id">{{ a.code ? a.code + ' · ' : '' }}{{ a.name }}</option>
        </template>
      </select>
    </label>
    <label>Commodity
      <select v-model="editor.commodity_id">
        <option v-for="c in commodities" :key="c.id" :value="c.id">{{ c.mnemonic }} ({{ c.namespace }})</option>
      </select>
    </label>
    <label>Description <input type="text" v-model="editor.description" /></label>
    <label class="check"><input type="checkbox" v-model="editor.placeholder" /> Placeholder</label>
  </div>
  <div class="acct-editor-hint">
    {{ editor.id ? 'Editing account' : 'New account' }} — Enter: save · Esc: cancel
  </div>
</div>

<div v-if="error" class="error-msg">{{ error }}</div>

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
      <tr v-for="(row, i) in rows" :key="row.acct.id"
          :class="{selected: i === selected, alt: i % 2 === 1}"
          @click="selected = i" @dblclick="open(row.acct.id)">
        <td class="mono">{{ row.acct.code ?? '' }}</td>
        <td>
          <span class="tree-indent">{{ indent(row.depth) }}{{ row.hasChildren ? (collapsed.has(row.acct.id) ? '▸ ' : '▾ ') : '' }}</span><span
            :class="{'placeholder-name': row.acct.placeholder}">{{ row.acct.name }}</span>
        </td>
        <td>{{ row.acct.kind }}</td>
        <td class="balance mono" :class="{neg: row.acct.balance.startsWith('-')}">{{ row.acct.balance }}</td>
        <td>{{ row.acct.commodity_id?.split(':')[1] ?? '' }}</td>
      </tr>
    </tbody>
  </table>
</div>

<div class="statusbar">
  <span><b>{{ rows.length }}</b> accounts</span>
  <span v-if="filter">filter: <b>{{ filter }}</b> (Esc clears)</span>
  <span>{{ hints.newItem }}: new · {{ hints.edit }}: edit · {{ hints.del }}: delete</span>
</div>
`,
}
