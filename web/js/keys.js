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
      addSplit: '⌃I',
      regNew: '⌃⏎',
      regInsert: '⌃I',
      regDup: '⌃D',
      regEdit: '⌃E',
      regSplits: '⌃S',
      regJump: '⌃J',
      regDelete: '⌘⌫',
      tabToggle: '⌃B',
      tabCycle: '⌃[ ⌃]',
      newReport: '⌃O',
      pdf: '⌃P',
    }
  : {
      newItem: 'Insert',
      edit: 'F2',
      del: 'Del',
      addSplit: 'Ctrl+I',
      regNew: 'Ctrl+Enter',
      regInsert: 'Ctrl+I',
      regDup: 'Ctrl+D',
      regEdit: 'Ctrl+E',
      regSplits: 'Ctrl+S',
      regJump: 'Ctrl+J',
      regDelete: 'Del',
      tabToggle: 'Ctrl+B',
      tabCycle: 'Ctrl+[ ]',
      newReport: 'Ctrl+O',
      pdf: 'Ctrl+P',
    }
