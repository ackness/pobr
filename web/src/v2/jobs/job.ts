import { jobEnded, jobStarted } from './activeJobs';

export interface JobProgress { done: number; total: number }
export interface JobState<T> { status: 'idle' | 'running' | 'done' | 'cancelled' | 'error'; progress: JobProgress | null; result: T | null; error: string | null }
export interface JobHandle { signal: AbortSignal; report: (done: number, total: number) => void }

const IDLE: JobState<never> = { status: 'idle', progress: null, result: null, error: null };

/**
 * One cancellable piece of long work. Starting again, or cancelling, abandons the previous run: its late
 * result is dropped rather than shown. Every in-flight job is counted so the top bar can say work is going on.
 */
export class JobController<T> {
  private state: JobState<T> = IDLE;
  private listeners = new Set<() => void>();
  private abort: AbortController | null = null;
  private run = 0;

  get = (): JobState<T> => this.state;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private set(next: JobState<T>) { this.state = next; this.listeners.forEach(listener => listener()); }
  private finish() { if (this.abort) { this.abort = null; jobEnded(); } }

  start = async (task: (job: JobHandle) => Promise<T>): Promise<void> => {
    this.cancel();
    const id = ++this.run;
    const controller = new AbortController();
    this.abort = controller; jobStarted();
    this.set({ status: 'running', progress: null, result: null, error: null });
    try {
      const result = await task({ signal: controller.signal, report: (done, total) => { if (id === this.run) this.set({ ...this.state, progress: { done, total } }); } });
      if (id !== this.run) return;
      this.finish(); this.set({ status: 'done', progress: null, result, error: null });
    } catch (error) {
      if (id !== this.run) return;
      this.finish();
      this.set({ status: controller.signal.aborted ? 'cancelled' : 'error', progress: null, result: null, error: controller.signal.aborted ? null : error instanceof Error ? error.message : String(error) });
    }
  };

  cancel = () => {
    if (this.state.status !== 'running' || !this.abort) return;
    this.run++;
    this.abort.abort();
    this.finish();
    this.set({ status: 'cancelled', progress: null, result: null, error: null });
  };

  reset = () => { this.cancel(); this.set(IDLE); };
}
