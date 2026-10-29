import { useEffect, useState } from 'react';

/** Width of an element in rem of the current root font size, so layout decisions follow the text-size preference. */
export function useShellRems(element: HTMLElement | null): number {
  const [rems, setRems] = useState(Infinity);
  useEffect(() => {
    if (!element) return;
    const measure = () => setRems(element.clientWidth / (parseFloat(getComputedStyle(document.documentElement).fontSize) || 16));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [element]);
  return rems;
}
