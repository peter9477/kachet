<script lang="ts">
  import { api, type Account } from './api'
  import { conn } from './conn.svelte'
  import AccountTree from './AccountTree.svelte'
  import Register from './Register.svelte'

  let accounts: Account[] = $state([])
  let error: string | null = $state(null)
  // Navigation: a stack of opened registers; empty = accounts view
  let stack: string[] = $state([])
  let current = $derived(stack.length ? stack[stack.length - 1] : null)

  async function reload() {
    try {
      accounts = await api.accounts()
      error = null
    } catch (e: any) {
      error = e.message
    }
  }
  reload()

  function openAccount(id: string) {
    stack = [...stack, id]
  }
  function goBack() {
    stack = stack.slice(0, -1)
  }
  function jumpAccount(id: string) {
    // Jump replaces (like GnuCash's Jump) but keeps history for Back
    stack = [...stack, id]
  }
</script>

<div class="topbar">
  <h1>kachet</h1>
  <span
    class="conn-dot"
    class:up={conn.up}
    title={conn.up ? 'Connected to backend' : 'Backend unreachable — reconnecting…'}
  >●</span>
  {#if current}
    {@const acct = accounts.find((a) => a.id === current)}
    <span>{acct?.name ?? ''}</span>
    <span class="hint">Esc/Backspace: back · ↑↓ PgUp PgDn: move · [ ]: month · &lbrace; &rbrace;: year · Space: splits · n: new · Enter: edit · j: jump · Del: delete</span>
  {:else}
    <span class="hint">↑↓: move · ←→: collapse/expand · Enter: open register · type to filter · Insert: new · F2: edit · Del: delete</span>
  {/if}
</div>

{#if error}
  <div class="error-msg">{error}</div>
{/if}

{#if current}
  {#key current + ':' + stack.length}
    <Register
      accountId={current}
      {accounts}
      onback={goBack}
      onjump={jumpAccount}
      onchanged={reload}
    />
  {/key}
{:else}
  <AccountTree {accounts} onopen={openAccount} onchanged={reload} />
{/if}
