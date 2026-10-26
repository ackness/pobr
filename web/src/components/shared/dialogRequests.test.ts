import { describe, expect, it } from 'vitest';
import { DialogRequests } from './dialogRequests';

describe('dialog request lifetime', () => {
  it('cancels active and queued requests when the stage scope ends', async () => {
    const scope = new DialogRequests();
    const confirmation = scope.open('confirm', 'Replace?', 'en-US');
    const prompt = scope.open('prompt', 'Name', 'zh-TW', 'Stage');
    const stale = scope.snapshot()!;
    scope.dispose();
    scope.finish(stale, true);
    expect(await confirmation).toBe(false);
    expect(await prompt).toBeNull();
    expect(await scope.open('confirm', 'Late request', 'en-US')).toBe(false);
  });
  it('settles each queued action once and preserves prompt text', async () => {
    const scope = new DialogRequests();
    const first = scope.open('confirm', 'Remove?', 'en-US');
    const second = scope.open('prompt', 'Name', 'zh-CN', '原名称');
    const firstRequest = scope.snapshot()!;
    scope.finish(firstRequest, true);
    scope.finish(firstRequest, false);
    expect(await first).toBe(true);
    expect(scope.snapshot()?.defaultValue).toBe('原名称');
    scope.finish(scope.snapshot()!, '新名称');
    expect(await second).toBe('新名称');
    expect(scope.snapshot()).toBeNull();
  });
});
