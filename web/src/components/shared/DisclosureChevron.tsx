/** Decorative counterpart of the native summary marker in app/control-consistency.css. */
export function DisclosureChevron({ className = '' }: { className?: string }) {
  return <span className={`disclosure-caret ${className}`.trim()} aria-hidden="true" />;
}
