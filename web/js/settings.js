// Client-side view of server settings (/api/settings). Reactive so
// computed values (e.g. fiscal periods) update when settings change.
// Defaults here apply until the server has a stored value.
import { reactive } from 'vue'
import { api } from './api.js'

export const settings = reactive({
  fiscal_year_end_month: 7, // fiscal years end at the end of this month
})

export async function loadSettings() {
  try {
    Object.assign(settings, await api.settings())
  } catch {
    // offline at boot: defaults stand; conn banner covers the rest
  }
}

export async function saveSettings(patch) {
  await api.saveSettings(patch)
  Object.assign(settings, patch)
}
