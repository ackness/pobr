import { useEffect, useRef, useState, type FocusEvent } from 'react';
import type { PassiveNode } from '../../api/types';

/** Pointer movement must not dismiss or replace the card holding keyboard focus. */
export function useTreeHover() {
  const [hover, setNode] = useState<PassiveNode | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const focused = useRef(false);
  const pointed = useRef(false);
  const cancel = () => { if (timer.current !== null) clearTimeout(timer.current); timer.current = null; };
  useEffect(() => cancel, []);
  const setHover = (node: PassiveNode | null) => {
    if (node === null) { focused.current = false; pointed.current = false; cancel(); }
    setNode(node);
  };
  const keepHover = () => { pointed.current = true; cancel(); };
  const leaveHover = () => {
    pointed.current = false;
    cancel();
    if (!focused.current) timer.current = setTimeout(() => {
      timer.current = null;
      if (!focused.current && !pointed.current) setHover(null);
    }, 200);
  };
  const hoverNode = (node: PassiveNode) => { if (!focused.current) { keepHover(); setHover(node); } };
  const onFocus = () => { focused.current = true; cancel(); };
  const onBlur = (event: FocusEvent<HTMLDivElement>) => {
    if (event.currentTarget.contains(event.relatedTarget)) return;
    focused.current = false;
    if (!pointed.current) leaveHover();
  };
  return { hover, setHover, hoverNode, keepHover, leaveHover, onFocus, onBlur };
}
