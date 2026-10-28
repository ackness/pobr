import type { ReactNode } from 'react';

/** Body positions around the figure, as the game's equipment screen lays them out. */
const BODY_SLOTS = ['weapon1', 'helmet', 'weapon2', 'amulet', 'ring1', 'bodyarmour', 'ring2', 'gloves', 'belt', 'boots'] as const;

/** A faint figure the slots are arranged around; it shows through the translucent slots. Decorative only. */
function Silhouette() {
  return <svg className="doll-silhouette" viewBox="0 0 100 100" aria-hidden="true" focusable="false" preserveAspectRatio="none">
    <g fill="currentColor">
      <circle cx="50" cy="11" r="8.5" />
      <path d="M41 19 H59 L61 25 Q82 27 86 38 L90 72 Q90 75 86 75 L81 74 L77 46 L68 44 L66 66 H34 L32 44 L23 46 L19 74 L14 75 Q10 75 10 72 L14 38 Q18 27 39 25 Z" />
      <path d="M35 64 H65 L63 100 H52 L50 74 L48 100 H37 Z" />
    </g>
  </svg>;
}

interface Props {
  /** Renders the button for one slot; the page owns selection, names and drafts. */
  slot: (id: string) => ReactNode;
  utilitySlots: readonly string[];
  limbSlots: readonly string[];
  /** Limbs are an ascendancy feature: they stay folded away until the build has one. */
  limbsInUse: boolean;
  labels: { body: string; utility: string; limbs: string };
}

/** The equipment screen: a figure with slots where the body is, then flasks and charms, then limbs. */
export function PaperDoll({ slot, utilitySlots, limbSlots, limbsInUse, labels }: Props) {
  return <div className="doll">
    <div className="paper-doll doll-figure" role="group" aria-label={labels.body}>
      <Silhouette />
      {BODY_SLOTS.map(id => <div key={id} className="doll-cell" data-slot={id}>{slot(id)}</div>)}
    </div>
    <div className="doll-group" role="group" aria-label={labels.utility}>
      <span className="doll-group-title" aria-hidden="true">{labels.utility}</span>
      <div className="utility-row doll-utility">{utilitySlots.map(id => slot(id))}</div>
    </div>
    <details className="doll-limbs" open={limbsInUse}>
      <summary>{labels.limbs}</summary>
      <div className="utility-row limb-row" role="group" aria-label={labels.limbs}>{limbSlots.map(id => slot(id))}</div>
    </details>
  </div>;
}
