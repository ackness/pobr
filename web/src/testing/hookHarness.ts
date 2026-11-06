type Cleanup = () => void;
type Effect = () => void | Cleanup;
type Deps = readonly unknown[] | undefined;
type Ref = { current: unknown };
type EffectEntry = { deps: Deps; cleanup?: Cleanup };
type MemoEntry = { deps: Deps; value: unknown };

interface HookHarnessOptions {
  lazyState?: boolean;
  functionalUpdates?: boolean;
  separateCursors?: boolean;
  refs?: boolean | {
    reuse?: 'present' | 'nullish';
    initialize?: (initial: unknown, index: number) => unknown;
    onRead?: (ref: Ref) => void;
  };
  memo?: 'eager' | 'deps';
  callback?: 'identity' | 'deps';
  effects?: 'ignore' | 'collect' | 'latest' | 'once' | {
    comparison: 'strict' | 'objectIs';
    cleanup?: 'latest' | 'previous';
  };
  captureEffects?: () => boolean;
  layoutEffect?: 'ignore' | 'effect';
}

// Compare only the supplied dependencies, as the original node-only mocks did.
function changed(deps: Deps, previous: Deps, comparison: 'strict' | 'objectIs') {
  return !deps || deps.some((value, index) => comparison === 'strict'
    ? value !== previous?.[index]
    : !Object.is(value, previous?.[index]));
}

/** Retain hook slots between explicit renders, overriding only the requested hooks. */
export function createHookHarness(options: HookHarnessOptions = {}) {
  const harness = {
    slots: [] as unknown[],
    refs: [] as Ref[],
    effectSlots: [] as EffectEntry[],
    cleanups: [] as (Cleanup | undefined)[],
    effects: [] as Effect[],
    effect: undefined as Effect | undefined,
    cleanup: undefined as Cleanup | undefined,
    cursor: 0,
    refIndex: 0,
    effectIndex: 0,
    dirty: false,
    beginRender() {
      harness.cursor = 0;
      harness.refIndex = 0;
      harness.effectIndex = 0;
      harness.dirty = false;
    },
    reset() {
      harness.slots = [];
      harness.refs = [];
      harness.effectSlots = [];
      harness.cleanups = [];
      harness.effects = [];
      harness.effect = undefined;
      harness.cleanup = undefined;
      harness.beginRender();
    },
    runEffects() {
      return harness.effects.splice(0).map(effect => effect());
    },
    mockReact(actual: object = {}) {
      const mock: Record<string, unknown> = {
        ...actual,
        useState: (initial: unknown) => {
          const index = harness.cursor++;
          if (!(index in harness.slots)) {
            harness.slots[index] = options.lazyState !== false && typeof initial === 'function' ? initial() : initial;
          }
          return [harness.slots[index], (value: unknown) => {
            harness.slots[index] = options.functionalUpdates !== false && typeof value === 'function'
              ? value(harness.slots[index]) : value;
            harness.dirty = true;
          }];
        },
      };
      if (options.refs) {
        const refs = typeof options.refs === 'object' ? options.refs : {};
        mock.useRef = (initial: unknown) => {
          const refIndex = harness.refIndex++;
          const index = options.separateCursors ? refIndex : harness.cursor++;
          const slots = options.separateCursors ? harness.refs : harness.slots;
          if (refs.reuse === 'nullish' ? slots[index] == null : !(index in slots)) {
            slots[index] = { current: refs.initialize ? refs.initialize(initial, refIndex) : initial };
          }
          const ref = slots[index] as Ref;
          refs.onRead?.(ref);
          return ref;
        };
      }
      const memo = (factory: () => unknown, deps: Deps) => {
        const index = harness.cursor++;
        const previous = harness.slots[index] as MemoEntry | undefined;
        if (!previous || changed(deps, previous.deps, 'objectIs')) {
          harness.slots[index] = { deps, value: factory() };
        }
        return (harness.slots[index] as MemoEntry).value;
      };
      if (options.memo) mock.useMemo = options.memo === 'eager' ? (factory: () => unknown) => factory() : memo;
      if (options.callback) mock.useCallback = options.callback === 'identity'
        ? (callback: unknown) => callback
        : (callback: unknown, deps: Deps) => memo(() => callback, deps);

      const effect = (callback: Effect, deps: Deps) => {
        if (options.effects === 'ignore') return;
        if (options.effects === 'latest') { harness.effect = callback; return; }
        if (options.effects === 'once') { harness.cleanup ??= callback() || undefined; return; }
        if (options.effects === 'collect') {
          if (!options.captureEffects || options.captureEffects()) harness.effects.push(callback);
          return;
        }
        if (!options.effects) return;
        const config = options.effects;
        const index = options.separateCursors ? harness.effectIndex++ : harness.cursor++;
        const slots = options.separateCursors ? harness.effectSlots : harness.slots;
        const previous = slots[index] as EffectEntry | undefined;
        if (previous && !changed(deps, previous.deps, config.comparison)) return;
        const entry: EffectEntry = { deps };
        slots[index] = entry;
        if (config.cleanup === 'previous') {
          harness.effects.push(() => { previous?.cleanup?.(); entry.cleanup = callback() || undefined; });
        } else if (config.cleanup === 'latest') {
          harness.effects.push(() => { harness.cleanups[index]?.(); harness.cleanups[index] = callback() || (() => {}); });
        } else {
          harness.effects.push(callback);
        }
      };
      if (options.effects) mock.useEffect = effect;
      if (options.layoutEffect) mock.useLayoutEffect = options.layoutEffect === 'ignore' ? () => {} : effect;
      return mock;
    },
  };
  return harness;
}
