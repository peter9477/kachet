import { ref, onMounted, onUnmounted } from 'vue'
import { hints } from './keys.js'
import { settings, saveSettings } from './settings.js'

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December']

export default {
  name: 'SettingsTab',
  props: { active: { type: Boolean, default: true } },
  emits: ['back'],
  setup(props, { emit }) {
    const fyMonth = ref(String(settings.fiscal_year_end_month))
    const status = ref('')
    const error = ref(null)

    async function save() {
      try {
        await saveSettings({ fiscal_year_end_month: Number(fyMonth.value) })
        error.value = null
        status.value = 'saved'
        setTimeout(() => (status.value = ''), 2000)
      } catch (e) {
        error.value = e.message
      }
    }

    function onkeydown(e) {
      if (!props.active) return
      if (e.key === 'Escape') {
        if (['INPUT', 'SELECT'].includes(document.activeElement?.tagName)) {
          document.activeElement.blur()
        } else {
          emit('back')
        }
        e.preventDefault()
      } else if (e.ctrlKey && !e.altKey && !e.metaKey && e.code === 'KeyS') {
        save()
        e.preventDefault()
      }
    }
    onMounted(() => window.addEventListener('keydown', onkeydown))
    onUnmounted(() => window.removeEventListener('keydown', onkeydown))

    return { MONTHS, fyMonth, status, error, hints, save }
  },
  template: `
<div class="pane">
  <div v-if="error" class="error-msg">{{ error }}</div>
  <div class="scroll">
    <div class="report-doc">
      <h2>Settings</h2>
      <div class="settings-grid">
        <label>Fiscal year ends at the end of
          <select v-model="fyMonth">
            <option v-for="(m, i) in MONTHS" :key="i" :value="String(i + 1)">{{ m }}</option>
          </select>
        </label>
        <div class="settings-note">
          Drives the FY/quarter picker in reports. Stored in the book's database.
        </div>
        <div>
          <button @click="save">Save</button>
          <span class="settings-status">{{ status }}</span>
        </div>
      </div>
    </div>
  </div>
  <div class="statusbar">
    <span></span>
    <span class="hint break-line">Esc: back · {{ hints.regSplits }}: save · {{ hints.tabToggle }}: tabs · {{ hints.tabCycle }}: switch tab</span>
  </div>
</div>
`,
}
