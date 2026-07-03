// Platform detection for keyboard-hint labels. The bindings themselves
// accept both variants everywhere; only the displayed hints differ.

export const isMac = ((navigator.userAgentData?.platform ?? navigator.platform) || '')
  .toLowerCase()
  .includes('mac')

export const hints = isMac
  ? {
      newItem: '⌃N',
      edit: '⌃E',
      del: '⌘⌫',
      addSplit: '⌥S',
      regDelete: 'd',
    }
  : {
      newItem: 'Insert',
      edit: 'F2',
      del: 'Del',
      addSplit: 'Insert / Alt+S',
      regDelete: 'd/Del',
    }
