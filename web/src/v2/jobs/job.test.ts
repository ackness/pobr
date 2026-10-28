import { describe, expect, it } from 'vitest';
import { activeJobCount } from './activeJobs';
import { JobController } from './job';

const deferred = <T,>() => { let resolve!: (value: T) => void; let reject!: (error: unknown) => void; const promise = new Promise<T>((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };

describe('JobController', () => {
  it('runs to a result, reports progress, and counts as active only while running', async () => {
    const job = new JobController<number>(); const gate = deferred<number>();
    const run = job.start(async ({ report }) => { report(1, 4); return gate.promise; });
    expect(job.get().status).toBe('running'); expect(activeJobCount()).toBe(1);
    await Promise.resolve();
    expect(job.get().progress).toEqual({ done: 1, total: 4 });
    gate.resolve(42); await run;
    expect(job.get()).toMatchObject({ status: 'done', result: 42, progress: null }); expect(activeJobCount()).toBe(0);
  });

  it('cancelling aborts the task and drops its late result', async () => {
    const job = new JobController<string>(); const gate = deferred<string>(); let aborted = false;
    const run = job.start(({ signal }) => { signal.addEventListener('abort', () => { aborted = true; }); return gate.promise; });
    job.cancel();
    expect(aborted).toBe(true); expect(job.get().status).toBe('cancelled'); expect(activeJobCount()).toBe(0);
    gate.resolve('late'); await run;
    expect(job.get()).toMatchObject({ status: 'cancelled', result: null });
  });

  it('starting again abandons the previous run', async () => {
    const job = new JobController<string>(); const first = deferred<string>(); const second = deferred<string>();
    const a = job.start(() => first.promise); const b = job.start(() => second.promise);
    first.resolve('old'); await a;
    expect(job.get().status).toBe('running');
    second.resolve('new'); await b;
    expect(job.get()).toMatchObject({ status: 'done', result: 'new' }); expect(activeJobCount()).toBe(0);
  });

  it('reports a failure without leaving the job counted', async () => {
    const job = new JobController<string>();
    await job.start(async () => { throw new Error('boom'); });
    expect(job.get()).toMatchObject({ status: 'error', error: 'boom' }); expect(activeJobCount()).toBe(0);
  });
});
