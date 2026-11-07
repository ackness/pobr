import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type RefObject } from 'react';
import { SIDE_DEFAULT, clampSideWidth, sideWidthForKey, sideWidthLimits } from './sidebarWidth';

/** The panel's width in rem, and the root font size in px that turns pointer pixels into rem. */
function measure(panel: HTMLElement | null) {
  const rootPx = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  return { rootPx, panelRem: panel ? panel.clientWidth / rootPx : Infinity };
}

interface Props {
  /** The `.tree-panel` that owns `--tree-side-preference`. */
  panelRef: RefObject<HTMLElement | null>;
  /** The committed width in rem. */
  width: number;
  label: string;
  onCommit: (rem: number) => void;
}

/**
 * Drag handle on the left edge of the side column. While dragging, the width goes straight into the panel's
 * CSS variable (one write per frame); React and storage hear about it once, on release.
 */
export function SidebarResizeHandle({ panelRef, width, label, onCommit }: Props) {
  const [panelRem, setPanelRem] = useState(Infinity);
  const drag = useRef<{ startX: number; startRem: number; rootPx: number; panelRem: number; pending: number; frame: number } | null>(null);

  useEffect(() => {
    const panel = panelRef.current;
    if (!panel || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => setPanelRem(measure(panel).panelRem));
    observer.observe(panel);
    return () => observer.disconnect();
  }, [panelRef]);
  // The last committed width, for undoing a drag that is cut short.
  const committed = useRef(width);
  committed.current = width;
  // Closing the column mid-drag: drop the drag marker and the half-dragged width.
  useEffect(() => {
    const panel = panelRef.current;
    return () => {
      if (!drag.current) return;
      cancelAnimationFrame(drag.current.frame);
      drag.current = null;
      if (panel) { delete panel.dataset.resizing; panel.style.setProperty('--tree-side-preference', `${committed.current}rem`); }
    };
  }, [panelRef]);

  const { min, max } = sideWidthLimits(panelRem);
  const value = clampSideWidth(width, panelRem);
  const paint = (rem: number) => panelRef.current?.style.setProperty('--tree-side-preference', `${rem}rem`);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const panel = panelRef.current;
    if (event.button !== 0 || !panel || drag.current) return;
    const { rootPx, panelRem: now } = measure(panel);
    event.currentTarget.setPointerCapture(event.pointerId);
    panel.dataset.resizing = '';
    const startRem = clampSideWidth(width, now);
    drag.current = { startX: event.clientX, startRem, rootPx, panelRem: now, pending: startRem, frame: 0 };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current) return;
    // The column sits on the right, so dragging left makes it wider.
    current.pending = clampSideWidth(current.startRem + (current.startX - event.clientX) / current.rootPx, current.panelRem);
    if (!current.frame) current.frame = requestAnimationFrame(() => { current.frame = 0; paint(current.pending); });
  };
  const finish = (event: PointerEvent<HTMLDivElement>, commit: boolean) => {
    const current = drag.current;
    if (!current) return;
    cancelAnimationFrame(current.frame);
    drag.current = null;
    delete panelRef.current?.dataset.resizing;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    // A click, or a drag back to where it started, changes nothing: restore the stored preference
    // (CSS caps it to the panel) instead of writing its clamped value.
    if (commit && current.pending !== current.startRem) {
      paint(current.pending);
      onCommit(current.pending);
    } else paint(committed.current);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const next = sideWidthForKey(event, value, panelRem);
    if (next === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    if (next !== value) onCommit(next);
  };

  return <div className="tree-side-resize" role="separator" aria-orientation="vertical" aria-controls="tree-side" aria-label={label}
    aria-valuenow={value} aria-valuemin={min} aria-valuemax={max} tabIndex={0}
    onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={event => finish(event, true)} onPointerCancel={event => finish(event, false)} onLostPointerCapture={event => finish(event, false)}
    onKeyDown={onKeyDown} onDoubleClick={() => onCommit(SIDE_DEFAULT)} />;
}
