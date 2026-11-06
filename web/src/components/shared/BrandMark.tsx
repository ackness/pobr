// The PoBR mark (owner's design, 2026-11-06). `detailed` adds the calculation glyphs in the bowl,
// which only read at about 32px and up; smaller marks use the simplified favicon form.
const LETTER = 'M 35,11 H 60 C 79,11 90,22 90,40 C 90,59 77,70 58,70 H 42 L 33,89 H 13 L 35,41 H 23 L 32,17 Q 34,11 40,11 Z M 48,30 L 39,53 H 55 C 65,53 70,48 70,40 C 70,34 66,30 58,30 Z';
const BOLT = 'M 8,68 L 36,51 L 33,62 H 52 L 17,85 L 23,70 Z';
const GLYPHS: readonly (readonly [number, number, number, number])[] = [
  [49.5, 38.5, 7, 2.6], [51.7, 36.3, 2.6, 7], [60.5, 38.5, 6, 2.6], [47.8, 46, 6, 2.6], [58.5, 46, 6, 2.6],
];
// The letter and bolt span x 8–90, y 11–89 (centre 49, 50); this puts that centre on the tile's (50, 50).
// favicon.svg and apple-touch-icon.png use the same placement.
const PLACEMENT = 'translate(9.82 9) scale(.82)';
const GOLD = '#d8b66f';
const INK = '#141516';

export function BrandMark({ className, detailed = false }: { className?: string; detailed?: boolean }) {
  return <svg className={className} viewBox="0 0 100 100" aria-hidden="true" focusable="false">
    <rect x="2" y="2" width="96" height="96" rx="23" fill={GOLD} />
    <g transform={PLACEMENT}>
      <path d={LETTER} fill={INK} fillRule="evenodd" />
      {detailed && <g fill={INK}>{GLYPHS.map(([x, y, width, height]) =>
        <rect key={`${x},${y}`} x={x} y={y} width={width} height={height} rx="1.3" />)}</g>}
      <path d={BOLT} fill={GOLD} />
    </g>
  </svg>;
}
