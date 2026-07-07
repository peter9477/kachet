import { createApp, ref, computed, watch } from 'vue'
import { api } from './api.js'
import { conn } from './conn.js'
import { hints } from './keys.js'
import { todayISO, fiscalYearOf, fiscalYear } from './fiscal.js'
import { loadSettings, settings } from './settings.js'
import AccountTree from './account-tree.js'
import Register from './register.js'
import Report from './report.js'
import SettingsTab from './settings-tab.js'

// Settings feed fiscal-period defaults; have them before first render.
await loadSettings()

const TAB_STORE = 'kachet-tabs'
let tabSeq = 1

const REPORT_KINDS = {
  'balance-sheet': { label: 'Balance Sheet', defaults: () => ({ date: todayISO() }) },
  'income-statement': { label: 'Income Statement', defaults: () => fiscalYear(fiscalYearOf()) },
  account: {
    label: 'Account Report',
    defaults: () => ({
      accounts: [],
      subaccounts: '1',
      from: '',
      to: '',
      dir: 'all',
      reconciled: 'all',
      filter: '',
    }),
  },
}

const App = {
  components: { AccountTree, Register, Report, SettingsTab },
  setup() {
    const accounts = ref([])
    const reportConfigs = ref([])
    const error = ref(null)
    // Tabs: the Accounts tree is a permanent first tab; registers and
    // reports open as closable tabs. Every tab stays mounted so each
    // keeps its own place; v-show picks the active one.
    // Register tabs hold a stack of account ids: "jump" pushes, Esc pops.
    // Report tabs hold {reportKind, name, params, configId?}.
    const tabs = ref([{ key: 'accounts', kind: 'accounts' }])
    const active = ref('accounts')
    const sidebar = ref(true)
    // Report launcher overlay (Ctrl+O): keyboard path to saved reports.
    const launcher = ref(null) // {sel: number} | null
    const activeTab = computed(() => tabs.value.find((t) => t.key === active.value))

    const tabTitle = (t) =>
      t.kind === 'accounts'
        ? 'Accounts'
        : t.kind === 'settings'
          ? 'Settings'
          : t.kind === 'report'
            ? t.name
            : (accounts.value.find((a) => a.id === t.stack[t.stack.length - 1])?.name ?? '…')

    // ---- persistence: restore the whole UI after a shutdown ----
    // Stored server-side (settings key "ui_state") so it survives across
    // browsers/machines; legacy localStorage is a one-time fallback.
    let restored = false
    function restoreTabs() {
      restored = true
      try {
        const saved =
          settings.ui_state ?? JSON.parse(localStorage.getItem(TAB_STORE) ?? 'null')
        const ids = new Set(accounts.value.map((a) => a.id))
        for (const t of saved.tabs ?? []) {
          if (t.kind === 'register') {
            const stack = (t.stack ?? []).filter((id) => ids.has(id))
            if (stack.length) {
              tabs.value.push({
                key: 'r' + tabSeq++,
                kind: 'register',
                stack,
                views: t.views ?? {},
              })
            }
          } else if (t.kind === 'settings') {
            tabs.value.push({ key: 'r' + tabSeq++, kind: 'settings' })
          } else if (t.kind === 'report' && REPORT_KINDS[t.reportKind]) {
            tabs.value.push({
              key: 'r' + tabSeq++,
              kind: 'report',
              reportKind: t.reportKind,
              name: t.name ?? REPORT_KINDS[t.reportKind].label,
              params: t.params ?? REPORT_KINDS[t.reportKind].defaults(),
              configId: t.configId ?? null,
            })
          }
        }
        if (Number.isInteger(saved.active) && tabs.value[saved.active]) {
          active.value = tabs.value[saved.active].key
        }
        if (saved.sidebar === false) sidebar.value = false
      } catch {}
    }
    function uiState() {
      return {
        tabs: tabs.value.map((t) =>
          t.kind === 'report'
            ? { kind: t.kind, reportKind: t.reportKind, name: t.name, params: t.params, configId: t.configId }
            : t.kind === 'register'
              ? { kind: t.kind, stack: t.stack, views: t.views }
              : { kind: t.kind },
        ),
        active: tabs.value.findIndex((t) => t.key === active.value),
        sidebar: sidebar.value,
      }
    }
    let saveTimer = null
    watch(
      [tabs, active, sidebar],
      () => {
        if (!restored) return
        clearTimeout(saveTimer)
        saveTimer = setTimeout(() => {
          api.saveSettings({ ui_state: uiState() }).catch(() => {})
        }, 800)
      },
      { deep: true },
    )
    // Best-effort flush on shutdown (keepalive lets it outlive the page).
    window.addEventListener('beforeunload', () => {
      if (!restored) return
      clearTimeout(saveTimer)
      try {
        fetch('/api/settings', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ ui_state: uiState() }),
          keepalive: true,
        })
      } catch {}
    })

    async function reload() {
      try {
        accounts.value = await api.accounts()
        error.value = null
        if (!restored) restoreTabs()
      } catch (e) {
        error.value = e.message
      }
    }
    reload()

    async function loadConfigs() {
      try {
        reportConfigs.value = await api.reportConfigs()
      } catch (e) {
        error.value = e.message
      }
    }
    loadConfigs()

    function addTab(t) {
      tabs.value.push(t)
      active.value = t.key
    }

    function openAccount(id) {
      const existing = tabs.value.find(
        (t) => t.kind === 'register' && t.stack[t.stack.length - 1] === id,
      )
      if (existing) {
        active.value = existing.key
        return
      }
      addTab({ key: 'r' + tabSeq++, kind: 'register', stack: [id], views: {} })
    }

    function newReport(kind) {
      const def = REPORT_KINDS[kind]
      addTab({
        key: 'r' + tabSeq++,
        kind: 'report',
        reportKind: kind,
        name: def.label,
        params: def.defaults(),
        configId: null,
      })
    }

    function openReportConfig(cfg) {
      const existing = tabs.value.find((t) => t.kind === 'report' && t.configId === cfg.id)
      if (existing) {
        active.value = existing.key
        return
      }
      addTab({
        key: 'r' + tabSeq++,
        kind: 'report',
        reportKind: cfg.kind,
        name: cfg.name,
        params: { ...cfg.params },
        configId: cfg.id,
      })
    }

    function openSettings() {
      const existing = tabs.value.find((t) => t.kind === 'settings')
      if (existing) active.value = existing.key
      else addTab({ key: 'r' + tabSeq++, kind: 'settings' })
    }

    function goBack(t) {
      if (t.kind === 'register' && t.stack.length > 1) t.stack.pop()
      else active.value = 'accounts'
    }

    function closeTab(t) {
      const i = tabs.value.indexOf(t)
      if (i < 0 || t.kind === 'accounts') return
      tabs.value.splice(i, 1)
      if (active.value === t.key) {
        active.value = tabs.value[Math.min(i, tabs.value.length - 1)].key
      }
    }

    function cycleTab(dir) {
      const n = tabs.value.length
      const i = tabs.value.findIndex((t) => t.key === active.value)
      active.value = tabs.value[(i + dir + n) % n].key
    }

    const fxStatus = ref('')
    async function fetchRates() {
      fxStatus.value = 'fetching rates…'
      try {
        const r = await api.fetchBocRates(todayISO())
        fxStatus.value = `added ${r.added} rate${r.added === 1 ? '' : 's'} (${r.series.join(', ') || 'no foreign currencies in use'})`
      } catch (e) {
        fxStatus.value = ''
        error.value = e.message
      }
    }

    // ---- report launcher (Ctrl+O) ----
    const launcherItems = computed(() => [
      ...Object.entries(REPORT_KINDS).map(([kind, def]) => ({
        label: `New ${def.label.toLowerCase()}…`,
        act: () => newReport(kind),
      })),
      {
        label: 'Fetch FX rates…',
        note: 'Bank of Canada',
        act: fetchRates,
      },
      {
        label: 'Settings…',
        act: openSettings,
      },
      ...reportConfigs.value.map((c) => ({
        label: c.name,
        note: REPORT_KINDS[c.kind]?.label ?? c.kind,
        act: () => openReportConfig(c),
      })),
    ])

    function launcherPick(i) {
      launcher.value = null
      launcherItems.value[i]?.act()
    }

    // App-level chords (capture phase so the modal launcher can swallow
    // keys before the per-view handlers see them).
    function onkeydown(e) {
      if (launcher.value) {
        const n = launcherItems.value.length
        if (e.key === 'ArrowDown') launcher.value.sel = (launcher.value.sel + 1) % n
        else if (e.key === 'ArrowUp') launcher.value.sel = (launcher.value.sel + n - 1) % n
        else if (e.key === 'Enter') launcherPick(launcher.value.sel)
        else if (e.key === 'Escape') launcher.value = null
        e.preventDefault()
        e.stopPropagation()
        return
      }
      if (!e.ctrlKey || e.altKey || e.metaKey) return
      if (e.code === 'KeyB') sidebar.value = !sidebar.value
      else if (e.code === 'KeyO') launcher.value = { sel: 0 }
      else if (e.code === 'BracketLeft') cycleTab(-1)
      else if (e.code === 'BracketRight') cycleTab(1)
      else return
      e.preventDefault()
      e.stopPropagation()
    }
    window.addEventListener('keydown', onkeydown, true)

    return {
      accounts, reportConfigs, error, conn, tabs, active, activeTab, sidebar, hints, fxStatus,
      launcher, launcherItems, launcherPick,
      tabTitle, reload, loadConfigs, openAccount, newReport, openReportConfig,
      goBack, closeTab,
      regKey: (t) => t.key + ':' + t.stack.length + ':' + t.stack[t.stack.length - 1],
      reloadPage: () => location.reload(),
    }
  },
  template: `
<div class="topbar">
  <span class="page-title">{{ activeTab ? tabTitle(activeTab) : '' }}</span>
  <span class="topbar-right">
    <span v-if="fxStatus" class="ver">{{ fxStatus }}</span>
    <h1>kachet</h1>
    <span v-if="conn.version" class="ver">v{{ conn.version }}</span>
    <span class="conn-dot" :class="{up: conn.up}"
      :title="conn.up ? 'Connected to backend' : 'Backend unreachable — reconnecting…'">●</span>
  </span>
</div>

<div v-if="conn.updateAvailable" class="update-banner">
  kachet was updated on the server —
  <button @click="reloadPage">reload now</button>
  or press Ctrl+R / F5 whenever convenient. Unsaved edits are lost on reload.
</div>

<div v-if="error" class="error-msg">{{ error }}</div>

<div class="main">
  <nav v-if="sidebar" class="sidebar">
    <div class="sidebar-head">
      <span>Tabs</span>
      <button class="sidebar-toggle" @click="sidebar = false" :title="'Collapse (' + hints.tabToggle + ')'">«</button>
    </div>
    <div v-for="t in tabs" :key="t.key" class="tab" :class="{active: t.key === active}"
        @click="active = t.key">
      <span class="tab-title">{{ tabTitle(t) }}</span>
      <button v-if="t.kind !== 'accounts'" class="tab-close" tabindex="-1"
        @click.stop="closeTab(t)" title="Close tab">×</button>
    </div>
    <div class="sidebar-head sidebar-reports-head">
      <span>Reports</span>
      <button class="sidebar-toggle" @click="launcher = {sel: 0}"
        :title="'New report (' + hints.newReport + ')'">+</button>
    </div>
    <div v-for="c in reportConfigs" :key="c.id" class="tab" @click="openReportConfig(c)">
      <span class="tab-title">{{ c.name }}</span>
    </div>
  </nav>
  <button v-else class="sidebar-collapsed" @click="sidebar = true"
    :title="'Show tabs (' + hints.tabToggle + ')'">»</button>

  <div class="content">
    <template v-for="t in tabs" :key="t.key">
      <div class="pane" v-show="t.key === active">
        <AccountTree v-if="t.kind === 'accounts'"
          :accounts="accounts" :active="t.key === active && !launcher"
          @open="openAccount" @changed="reload" />
        <SettingsTab v-else-if="t.kind === 'settings'"
          :active="t.key === active && !launcher" @back="active = 'accounts'" />
        <Report v-else-if="t.kind === 'report'"
          :tab="t" :accounts="accounts" :active="t.key === active && !launcher"
          @open="openAccount" @back="goBack(t)" @configs-changed="loadConfigs" />
        <Register v-else :key="regKey(t)"
          :account-id="t.stack[t.stack.length - 1]" :accounts="accounts"
          :views="t.views" :active="t.key === active && !launcher"
          @back="goBack(t)" @jump="(id) => t.stack.push(id)" @changed="reload" />
      </div>
    </template>
  </div>
</div>

<div v-if="launcher" class="launcher-backdrop" @click="launcher = null">
  <div class="launcher" @click.stop>
    <div class="launcher-title">Reports</div>
    <div v-for="(item, i) in launcherItems" :key="i" class="launcher-item"
        :class="{selected: i === launcher.sel}"
        @mouseenter="launcher.sel = i" @click="launcherPick(i)">
      <span>{{ item.label }}</span>
      <span v-if="item.note" class="launcher-note">{{ item.note }}</span>
    </div>
    <div class="launcher-hint">↑↓ · Enter: open · Esc: close</div>
  </div>
</div>
`,
}

createApp(App).mount('#app')
