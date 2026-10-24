import { defineStore } from 'pinia';
import type { ProgressStep, ProgressStatus } from '@/types/progress';
import { ProgressDisplay } from '../utils/progressDisplay';

const displays = new WeakMap<object, ProgressDisplay>();

export const useProgressStore = defineStore('progress', {
    state: () => ({
        dialogOpen: false,
        steps: [] as ProgressStep[],
        displaySteps: [] as ProgressStep[],
        displayBusy: false,
        finishedStatus: null as 'success' | 'error' | 'cancelled' | null,
        currentStepId: null as string | null,
        errorMessage: '',
    }),

    actions: {
        reset() {
            displays.get(this)?.dispose();
            displays.delete(this);
            this.steps = [];
            this.displaySteps = [];
            this.displayBusy = false;
            this.finishedStatus = null;
            this.currentStepId = null;
            this.errorMessage = '';
            this.dialogOpen = false;
        },

        openDialog() {
            this.dialogOpen = true;
        },

        closeDialog() {
            this.dialogOpen = false;
        },

        async closeDialogAfterDisplay() {
            const display = displays.get(this);
            await display?.whenIdle();
            if (displays.get(this) === display && this.finishedStatus === 'success') {
                this.closeDialog();
            }
        },

        setSteps(steps: ProgressStep[]) {
            displays.get(this)?.dispose();
            this.steps = steps.map(step => ({ ...step }));
            this.finishedStatus = null;
            this.errorMessage = '';
            this.currentStepId = steps.find(step => step.status === 'running')?.id ?? null;
            displays.set(this, new ProgressDisplay(steps, (visible, busy) => {
                this.displaySteps = visible;
                this.displayBusy = busy;
            }));
        },

        updateStep(updatedStep: ProgressStep) {
            const index = this.steps.findIndex(step => step.id === updatedStep.id);
            if (index === -1) return;
            this.steps[index] = { ...this.steps[index], ...updatedStep };
            this.currentStepId = this.steps.find(step => step.status === 'running')?.id ?? null;
            if (updatedStep.status === 'error' && updatedStep.error) this.errorMessage = updatedStep.error;
            displays.get(this)?.update(this.steps[index]);
        },

        finish(success: boolean, message?: string) {
            const hasError = this.steps.some(step => step.status === 'error');
            const cancelled = !success && !hasError && (
                /取消|cancelled|canceled/i.test(message ?? '') ||
                (!message && this.steps.some(step => step.status === 'cancelled'))
            );
            this.finishedStatus = success && !hasError ? 'success' : cancelled ? 'cancelled' : 'error';
            if (cancelled) this.errorMessage = '';
            if (!success && !cancelled && message) this.errorMessage = message;
            let markedFailure = hasError;
            const hasRunning = this.steps.some(step => step.status === 'running');
            for (const step of [...this.steps]) {
                if (step.status !== 'running' && step.status !== 'pending') continue;
                let status: ProgressStatus = step.status === 'running' && success ? 'success' : 'cancelled';
                if (!success && !cancelled && !markedFailure && (step.status === 'running' || !hasRunning)) {
                    status = 'error';
                    markedFailure = true;
                }
                this.updateStep({ ...step, status, error: status === 'error' ? message : undefined });
            }
            if (this.finishedStatus === 'success' && message) {
                const last = this.steps[this.steps.length - 1];
                if (last) last.title = message;
                displays.get(this)?.setCompletionTitle(message);
            }
        },

        // Legacy updates use the same event and presentation path.
        updateStepStatus(id: string, status: ProgressStatus) {
            const step = this.steps.find(step => step.id === id);
            if (step) this.updateStep({ ...step, status });
        },

        setStepRunning(id: string) {
            this.updateStepStatus(id, 'running');
        },

        setStepSuccess(id: string) {
            this.updateStepStatus(id, 'success');
        },

        setStepError(id: string, message?: string) {
            const step = this.steps.find(step => step.id === id);
            if (step) this.updateStep({ ...step, status: 'error', error: message });
            if (message) this.errorMessage = message;
        },

        updateDownloadProgress(id: string, progress: number, speed: string, eta: string) {
            const step = this.steps.find(step => step.id === id);
            if (step?.type === 'download') {
                this.updateStep({ ...step, progress, downloadSpeed: speed, eta });
            }
        },

        // Mock helper for the demo
        async mockStartInstallation() {
            this.reset();
            this.setSteps([
                { id: 'step1', title: 'Checking Environment', status: 'pending', type: 'normal' },
                { id: 'step2', title: 'Downloading Firmware', status: 'pending', type: 'download', progress: 0, downloadSpeed: '0 MB/s', eta: '--' },
                { id: 'step3', title: 'Extracting Files', status: 'pending', type: 'normal' },
                { id: 'step4', title: 'Verifying Installation', status: 'pending', type: 'normal' },
            ]);
            this.openDialog();

            // Step 1: Check Env
            await this.sleep(1000);
            this.updateStepStatus('step1', 'running');
            await this.sleep(1500);
            this.updateStepStatus('step1', 'success');

            // Step 2: Download
            this.updateStepStatus('step2', 'running');
            for (let i = 0; i <= 100; i += 5) {
                await this.sleep(200);
                this.updateDownloadProgress('step2', i, `${(Math.random() * 5 + 2).toFixed(1)} MB/s`, `${Math.floor((100 - i) / 10)}s`);
            }
            this.updateStepStatus('step2', 'success');

            // Step 3: Extract
            this.updateStepStatus('step3', 'running');
            await this.sleep(2000);
            this.updateStepStatus('step3', 'success');

            // Step 4: Verify
            this.updateStepStatus('step4', 'running');
            await this.sleep(1000);
            // Mock random success or error for testing
            if (Math.random() > 0.7) {
                this.setStepError('step4', '验证失败：文件校验和不匹配。请检查下载的文件是否完整。');
            } else {
                this.updateStepStatus('step4', 'success');
            }
        },

        // Mock installation with guaranteed error for testing
        async mockStartInstallationWithError() {
            this.reset();
            this.setSteps([
                { id: 'step1', title: '检查环境', status: 'pending', type: 'normal' },
                { id: 'step2', title: '下载固件', status: 'pending', type: 'download', progress: 0, downloadSpeed: '0 MB/s', eta: '--' },
                { id: 'step3', title: '解压文件', status: 'pending', type: 'normal' },
            ]);
            this.openDialog();

            // Step 1: Success
            await this.sleep(800);
            this.updateStepStatus('step1', 'running');
            await this.sleep(1000);
            this.updateStepStatus('step1', 'success');

            // Step 2: Download with error
            this.updateStepStatus('step2', 'running');
            for (let i = 0; i <= 50; i += 10) {
                await this.sleep(200);
                this.updateDownloadProgress('step2', i, `${(Math.random() * 5 + 2).toFixed(1)} MB/s`, `${Math.floor((100 - i) / 10)}s`);
            }
            // Simulate download error
            this.setStepError('step2', '下载失败：网络连接超时。请检查您的网络连接后重试。');
        },

        sleep(ms: number) {
            return new Promise(resolve => setTimeout(resolve, ms));
        }
    }
});
