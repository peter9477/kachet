import { createApp, ref, computed } from 'vue'
import { api } from './api.js'
import { conn } from './conn.js'
import AccountTree from './account-tree.js'
import Register from './register.js'

const App = {
  components: { AccountTree, Register },
  setup() {
    const accounts = ref([])
    const error = ref(null)
    // Navigation: a stack of opened registers; empty = accounts view
    const stack = ref([])
    const current = computed(() => (stack.value.length ? stack.value[stack.value.length - 1] : null))
    const currentName = computed(() => accounts.value.find((a) => a.id === current.value)?.name ?? '')
    // Remount the register when navigating, even to the same account
    const regKey = computed(() => current.value + ':' + stack.value.length)

    async function reload() {
      try {
        accounts.value = await api.accounts()
        error.value = null
      } catch (e) {
        error.value = e.message
      }
    }
    reload()

    return {
      accounts, error, conn, current, currentName, regKey, reload,
      openAccount: (id) => (stack.value = [...stack.value, id]),
      goBack: () => (stack.value = stack.value.slice(0, -1)),
      reloadPage: () => location.reload(),
    }
  },
  template: `
<div class="topbar">
  <h1>kachet</h1>
  <template v-if="current">
    <span>{{ currentName }}</span>
    <span class="hint">Esc/Backspace: back · ↑↓ PgUp PgDn: move · [ ]: month · { }: year · Space: splits · n: new · Enter: edit · j: jump · Del: delete</span>
  </template>
  <template v-else>
    <span class="hint">↑↓: move · ←→: collapse/expand · Enter: open register · type to filter · Insert/⌃N: new · F2/⌃E: edit · Del/⌘⌫: delete</span>
  </template>
  <span class="topbar-right">
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

<Register v-if="current" :key="regKey"
  :account-id="current" :accounts="accounts"
  @back="goBack" @jump="openAccount" @changed="reload" />
<AccountTree v-else :accounts="accounts" @open="openAccount" @changed="reload" />
`,
}

createApp(App).mount('#app')
