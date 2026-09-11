/** Shared position symbols for equipment and market navigation. */
export function SlotSymbol({ slot }: { slot: string }) {
  const path = slot === 'gems' || slot.startsWith('Jewel@') ? 'M12 3 3 10l9 11 9-11-9-7Zm-9 7h18M8 6l4 15 4-15'
    : slot.startsWith('weapon') ? 'M5 3c17 2 17 16 0 18M5 3l5 9-5 9M3 12h18m-4-3 4 3-4 3'
    : slot.startsWith('ring') ? 'M8 5l4-3 4 3-4 4-4-4Zm1 4a7 7 0 1 0 6 0'
    : slot === 'amulet' ? 'M5 3c0 8 3 11 7 11s7-3 7-11M12 14l4 4-4 4-4-4 4-4Z'
    : slot === 'helmet' ? 'M5 19V10a7 7 0 0 1 14 0v9l-5 2v-7h-4v7l-5-2ZM5 12h5m4 0h5'
    : slot === 'gloves' ? 'M7 21 4 12a2 2 0 0 1 3-2l2 3V5a1 1 0 0 1 2 0v5-7h2v7-6h2v7-5h2v10l-3 5H7Z'
    : slot === 'boots' ? 'M8 3h9l-1 11 4 4v3H4v-4l5-4L8 3Zm1 5h7M4 17h10'
    : slot === 'belt' ? 'M3 8h18v9H3V8Zm6-1h6v11H9V7Zm3 5h5'
    : /^(Flask|Charm)/.test(slot) ? 'M9 3h6m-5 0v6L6 16v4h12v-4l-4-7V3M8 15h8'
    : 'M8 3 3 6l2 6 3-1v10h8V11l3 1 2-6-5-3c0 4-8 4-8 0Z';
  return <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d={path} /></svg>;
}
