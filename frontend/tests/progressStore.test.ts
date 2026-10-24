import { describe, expect, test } from 'bun:test';
import { createPinia } from 'pinia';
import { useProgressStore } from '../src/stores/ProgressStore';
import type { ProgressStep, ProgressStatus } from '../src/types/progress';

const step = (status: ProgressStatus, id = 'check_env'): ProgressStep => ({
    id, title: '检查运行环境', status, type: 'normal',
});
const waitForDisplay = () => new Promise(resolve => setTimeout(resolve, 350));

function createStore(status: ProgressStatus = 'pending') {
    const store = useProgressStore(createPinia());
    store.setSteps([step(status)]);
    return store;
}

describe('shared progress presentation', () => {
    test('back-to-back running and success events leave time to paint the spinner', async () => {
        const store = createStore();
        store.updateStep(step('running'));
        store.updateStep(step('success'));
        expect(store.displaySteps[0].status).toBe('running');
        expect(store.steps[0].status).toBe('success');
        expect(store.currentStepId).toBeNull();
        // A finished event can update the title while success is waiting to display.
        store.finish(true, '安装完成');
        await waitForDisplay();
        expect(store.displaySteps[0].status).toBe('success');
        expect(store.displaySteps[0].title).toBe('安装完成');
    });

    test('a slow check keeps running and completes without another delay', async () => {
        const store = createStore();
        store.updateStep(step('running'));
        await waitForDisplay();
        expect(store.displaySteps[0].status).toBe('running');
        store.updateStep(step('success'));
        expect(store.displaySteps[0].status).toBe('success');
    });

    for (const status of ['error', 'cancelled'] as const) {
        test(`${status} immediately replaces pending success and cannot be overwritten`, async () => {
            const store = createStore();
            store.updateStep(step('running'));
            store.updateStep(step('success'));
            store.updateStep({ ...step(status), error: '检查中断' });
            expect(store.displaySteps[0].status).toBe(status);
            expect(store.errorMessage).toBe(status === 'error' ? '检查中断' : '');
            await waitForDisplay();
            expect(store.displaySteps[0].status).toBe(status);
        });
    }

    for (const replace of ['reset', 'setSteps'] as const) {
        test(`${replace} prevents an old completion timer from finishing a new check`, async () => {
            const store = createStore();
            store.updateStep(step('running'));
            store.updateStep(step('success'));
            if (replace === 'reset') {
                store.reset();
                store.setSteps([step('running')]);
            } else {
                store.setSteps([step('running')]);
            }
            await waitForDisplay();
            expect(store.displaySteps[0].status).toBe('running');
            store.reset();
        });
    }

    test('initial running steps and legacy updates use the same display timing', async () => {
        const store = createStore('running');
        store.setStepSuccess('check_env');
        expect(store.displaySteps[0].status).toBe('running');
        await waitForDisplay();
        expect(store.displaySteps[0].status).toBe('success');
    });

    test('success without a running event stays immediate; other active steps use display timing', () => {
        const store = createStore();
        store.updateStep(step('success'));
        expect(store.displaySteps[0].status).toBe('success');
        store.setSteps([step('pending', 'install')]);
        store.updateStep(step('running', 'install'));
        store.updateStep(step('success', 'install'));
        expect(store.displaySteps[0].status).toBe('running');
        store.reset();
    });
    for (const id of ['install_aria2', 'check_exe', 'detect_version', 'load_keys', 'find_nca', 'extract_version', 'find_asset']) {
        test(`${id}: fast success preserves the spinner`, async () => {
            const store = useProgressStore(createPinia());
            store.setSteps([step('pending', id)]);
            store.updateStep(step('running', id));
            store.updateStep(step('success', id));
            expect(store.steps[0].status).toBe('success');
            expect(store.displaySteps[0].status).toBe('running');
            await waitForDisplay();
            expect(store.displaySteps[0].status).toBe('success');
        });
    }
    test('sequential fast phases display one spinner at a time', async () => {
        const store = createStore();
        store.setSteps([step('pending'), step('pending', 'install')]);
        for (const id of ['check_env', 'install']) {
            store.updateStep(step('running', id));
            store.updateStep(step('success', id));
        }
        expect(store.displaySteps.map(s => s.status)).toEqual(['running', 'pending']);
        await waitForDisplay();
        expect(store.displaySteps.map(s => s.status)).toEqual(['success', 'running']);
        await waitForDisplay();
        expect(store.displaySteps.map(s => s.status)).toEqual(['success', 'success']);
    });
    test('auto-close waits for display completion and never hides failure', async () => {
        const store = createStore('running');
        store.openDialog();
        store.setStepSuccess('check_env');
        store.finish(true);
        const closing = store.closeDialogAfterDisplay();
        expect(store.dialogOpen).toBe(true);
        await closing;
        expect(store.dialogOpen).toBe(false);
        store.setSteps([step('running'), step('pending', 'install')]);
        store.openDialog();
        store.finish(false, '检查失败');
        expect(store.displaySteps.map(s => s.status)).toEqual(['error', 'cancelled']);
        await store.closeDialogAfterDisplay();
        expect(store.dialogOpen).toBe(true);
        expect(store.errorMessage).toBe('检查失败');
    });
    test('an old auto-close cannot close a new operation', async () => {
        const store = createStore('running');
        store.setStepSuccess('check_env');
        store.finish(true);
        const closing = store.closeDialogAfterDisplay();
        store.reset();
        store.setSteps([step('running')]);
        store.openDialog();
        await closing;
        await waitForDisplay();
        expect(store.displaySteps[0].status).toBe('running');
        expect(store.dialogOpen).toBe(true);
        store.reset();
    });
    test('finished cancellation and late errors cannot leave a running spinner or show success', () => {
        const store = createStore('running');
        store.finish(false, '用户取消了操作');
        expect(store.displaySteps[0].status).toBe('cancelled');
        expect(store.finishedStatus).toBe('cancelled');
        store.setSteps([step('success')]);
        store.finish(false, '保存配置失败');
        expect(store.finishedStatus).toBe('error');
        expect(store.errorMessage).toBe('保存配置失败');
        store.setSteps([step('error')]);
        store.finish(true);
        expect(store.finishedStatus).toBe('error');
    });
    test('queued download progress coalesces while real progress stays current', async () => {
        const store = createStore();
        const download = { ...step('pending', 'download'), type: 'download' as const };
        store.setSteps([step('pending'), download]);
        store.updateStep(step('running'));
        store.updateStep(step('success'));
        for (let progress = 0; progress <= 100; progress++) store.updateStep({ ...download, status: 'running', progress });
        expect(store.steps[1].progress).toBe(100);
        await waitForDisplay();
        expect(store.displaySteps[1].progress).toBe(100);
        expect(store.displaySteps[1].status).toBe('running');
        store.reset();
    });

});
