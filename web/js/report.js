import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { api } from './api.js'
import { hints } from './keys.js'
import { fiscalPeriods } from './fiscal.js'

// A live report tab. `tab` is the app's tab object; params/name/configId
// are edited in place so the app's tab persistence picks them up.
export default {
  name: 'Report',
  props: {
    tab: { type: Object, required: true },
    accounts: { type: Array, default: () => [] },
    active: { type: Boolean, default: true },
  },
  emits: ['open', 'back', 'configs-changed'],
  setup(props, { emit }) {
    const rootEl = ref(null)
    const data = ref(null)
    const error = ref(null)
    const loading = ref(false)
    const periodSel = ref('')
    // Computed: tracks the fiscal-year-end setting reactively.
    const periods = computed(() => fiscalPeriods())

    // Flatten params for the query string: join arrays, drop empties.
    function flatParams() {
      const out = {}
      for (const [k, v] of Object.entries(props.tab.params)) {
        const s = Array.isArray(v) ? v.join(',') : v
        if (s !== '' && s != null) out[k] = s
      }
      return out
    }

    const needsAccounts = () =>
      props.tab.reportKind === 'account' && !(props.tab.params.accounts ?? []).length

    async function load() {
      if (needsAccounts()) {
        data.value = null
        error.value = null
        return
      }
      loading.value = true
      try {
        data.value = await api.report(props.tab.reportKind, flatParams())
        error.value = null
      } catch (e) {
        error.value = e.message
      }
      loading.value = false
    }
    load()

    watch(() => JSON.stringify(props.tab.params), (a, b) => {
      if (a !== b) load()
    })
    // Refresh when this tab is revisited: entries may have changed elsewhere.
    watch(() => props.active, (a) => {
      if (a && data.value) load()
    })

    const periodLabel = computed(() => {
      const p = props.tab.params
      if (props.tab.reportKind === 'balance-sheet') return p.date
      if (props.tab.reportKind === 'income-statement') return `${p.from} to ${p.to}`
      const range =
        p.from && p.to ? `${p.from} to ${p.to}`
        : p.from ? `from ${p.from}`
        : p.to ? `through ${p.to}`
        : 'all dates'
      const extras = [
        p.dir === 'in' ? 'debits' : p.dir === 'out' ? 'credits' : '',
        { y: 'reconciled', c: 'cleared', n: 'new', ny: 'unreconciled' }[p.reconciled] ?? '',
        p.filter ? `“${p.filter}”` : '',
      ].filter(Boolean)
      return [range, ...extras].join(' · ')
    })

    const pdfUrl = computed(() =>
      `/api/reports/${props.tab.reportKind}/pdf?` +
      new URLSearchParams({ ...flatParams(), title: props.tab.name }),
    )

    const selectableAccounts = computed(() =>
      [...props.accounts].sort(
        (x, y) => (x.code ?? '￿').localeCompare(y.code ?? '￿') || x.name.localeCompare(y.name),
      ),
    )

    function applyPeriod() {
      const p = periods.value.find((x) => x.label === periodSel.value)
      if (!p) return
      if (props.tab.reportKind === 'balance-sheet') props.tab.params.date = p.to
      else Object.assign(props.tab.params, { from: p.from, to: p.to })
    }

    async function save() {
      const body = {
        name: props.tab.name,
        kind: props.tab.reportKind,
        params: props.tab.params,
      }
      try {
        if (props.tab.configId) await api.updateReportConfig(props.tab.configId, body)
        else props.tab.configId = (await api.createReportConfig(body)).id
        error.value = null
        emit('configs-changed')
      } catch (e) {
        error.value = e.message
      }
    }

    function saveAs() {
      props.tab.configId = null
      save()
    }

    async function removeConfig() {
      if (!props.tab.configId) return
      if (!confirm(`Delete saved report "${props.tab.name}"?`)) return
      try {
        await api.deleteReportConfig(props.tab.configId)
        props.tab.configId = null
        error.value = null
        emit('configs-changed')
      } catch (e) {
        error.value = e.message
      }
    }

    function onkeydown(e) {
      if (!props.active) return
      const inField = ['INPUT', 'SELECT', 'TEXTAREA'].includes(document.activeElement?.tagName)
      if (e.key === 'Escape') {
        if (inField) document.activeElement.blur()
        else emit('back')
        e.preventDefault()
      } else if (e.ctrlKey && !e.altKey && !e.metaKey && e.code === 'KeyS') {
        save()
        e.preventDefault()
      } else if (e.ctrlKey && !e.altKey && !e.metaKey && e.code === 'KeyP') {
        window.open(pdfUrl.value, '_blank')
        e.preventDefault()
      }
    }
    onMounted(() => window.addEventListener('keydown', onkeydown))
    onUnmounted(() => window.removeEventListener('keydown', onkeydown))

    return {
      rootEl, data, error, loading, periodSel, periods, periodLabel, pdfUrl, hints,
      selectableAccounts, needsAccounts,
      applyPeriod, save, saveAs, removeConfig,
    }
  },
  template: `
<div class="pane" ref="rootEl">
  <div class="report-toolbar">
    <template v-if="tab.reportKind === 'balance-sheet'">
      <label>As of <input type="date" v-model="tab.params.date" /></label>
    </template>
    <template v-else>
      <label>From <input type="date" v-model="tab.params.from" /></label>
      <label>To <input type="date" v-model="tab.params.to" /></label>
    </template>
    <template v-if="tab.reportKind === 'account'">
      <label>Accounts
        <select multiple size="4" class="acct-multi" v-model="tab.params.accounts">
          <option v-for="a in selectableAccounts" :key="a.id" :value="a.id">{{ a.code ? a.code + ' · ' : '' }}{{ a.name }}</option>
        </select>
      </label>
      <label class="check"><input type="checkbox" true-value="1" false-value="0"
        v-model="tab.params.subaccounts" /> subaccounts</label>
      <label>Direction
        <select v-model="tab.params.dir">
          <option value="all">any</option>
          <option value="in">debits</option>
          <option value="out">credits</option>
        </select>
      </label>
      <label>Reconciled
        <select v-model="tab.params.reconciled">
          <option value="all">any</option>
          <option value="y">reconciled</option>
          <option value="ny">unreconciled</option>
          <option value="c">cleared</option>
          <option value="n">new</option>
        </select>
      </label>
      <label>Contains <input type="text" v-model.lazy="tab.params.filter" placeholder="text" /></label>
    </template>
    <select v-model="periodSel" @change="applyPeriod" title="Jump to fiscal period">
      <option value="">— fiscal period —</option>
      <option v-for="p in periods" :key="p.label" :value="p.label">{{ p.label }}</option>
    </select>
    <span class="toolbar-gap"></span>
    <input class="cfg-name" type="text" v-model="tab.name" placeholder="Report name" />
    <a class="toolbar-btn" :href="pdfUrl" target="_blank" :title="hints.pdf + ': PDF'">PDF</a>
    <button @click="save">{{ tab.configId ? 'Save' : 'Save report' }}</button>
    <button v-if="tab.configId" @click="saveAs" title="Save a copy under this name">Save as new</button>
    <button v-if="tab.configId" @click="removeConfig">Delete saved</button>
  </div>

  <div v-if="error" class="error-msg">{{ error }}</div>

  <div class="scroll report-scroll">
    <div class="report-doc">
      <h2>{{ tab.name }} <span class="report-period">{{ periodLabel }}</span></h2>
      <div v-if="needsAccounts()" class="report-prompt">Select one or more accounts above.</div>
      <template v-if="data && data.kind === 'account'">
        <div v-for="g in data.groups" :key="g.account_id" class="acct-group">
          <h3><a href="#" @click.prevent="$emit('open', g.account_id)">{{ g.account_name }}</a>
            <span class="report-period">({{ g.commodity }})</span></h3>
          <table class="report-table acct-list">
            <thead>
              <tr><th>Date</th><th>Num</th><th>Description</th><th>Memo</th><th>R</th>
                <th class="amount">Amount</th><th class="amount">Running</th></tr>
            </thead>
            <tbody>
              <tr v-for="r in g.rows" :key="r.tx_id + r.date + r.amount">
                <td class="mono">{{ r.date }}</td>
                <td class="mono">{{ r.num ?? '' }}</td>
                <td>{{ r.description ?? '' }}</td>
                <td>{{ r.memo ?? '' }}</td>
                <td>{{ r.reconcile_state }}</td>
                <td class="amount mono" :class="{neg: r.amount.startsWith('-')}">{{ r.amount }}</td>
                <td class="amount mono" :class="{neg: r.running.startsWith('-')}">{{ r.running }}</td>
              </tr>
              <tr class="r-total">
                <td colspan="5">Total {{ g.account_name }}</td>
                <td></td>
                <td class="amount mono r-figure" :class="{neg: g.total.startsWith('-')}">{{ g.total }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <div v-if="!data.groups.length" class="report-prompt">No matching entries.</div>
        <div v-else-if="data.grand_total != null && data.groups.length > 1" class="acct-grand mono">
          Grand total: <b :class="{neg: data.grand_total.startsWith('-')}">{{ data.grand_total }} {{ data.grand_currency }}</b>
        </div>
      </template>
      <table class="report-table" v-if="data && data.kind !== 'account'">
        <colgroup>
          <col />
          <col v-for="c in data.cols" :key="c" class="report-amt-col" />
        </colgroup>
        <tbody>
          <tr v-for="(r, i) in data.rows" :key="i" :class="'r-' + r.kind">
            <td class="r-label" :style="{ paddingLeft: (0.4 + r.depth * 1.4) + 'em' }">
              <a v-if="r.kind === 'account' && r.account_id" href="#"
                 @click.prevent="$emit('open', r.account_id)">{{ r.label }}</a>
              <template v-else>{{ r.label }}</template>
            </td>
            <td v-for="c in data.cols" :key="c" class="amount mono r-amt"
                :class="{'r-figure': c - 1 === r.col && r.amount, neg: c - 1 === r.col && r.amount && r.amount.startsWith('-')}">
              <template v-if="c - 1 === r.col && r.amount">{{ r.amount }}</template>
              <div v-if="c - 1 === r.col && r.foreign" class="r-foreign">{{ r.foreign }}</div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>

  <div class="statusbar">
    <span v-if="loading">computing…</span>
    <span v-else-if="data && data.kind === 'account'">{{ data.count }} entries</span>
    <span v-else-if="data">{{ data.currency }}</span>
    <span class="hint break-line">Esc: back · {{ hints.regSplits }}: save · {{ hints.pdf }}: PDF · {{ hints.newReport }}: reports · {{ hints.tabToggle }}: tabs · {{ hints.tabCycle }}: switch tab</span>
  </div>
</div>
`,
}
