import { defineStore } from 'pinia';
import type { InstallationStep, InstallationStatus } from '@/types/installation';

export const useInstallationStore = defineStore('installation', {
    state: () => ({
        dialogOpen: false,
        steps: [] as InstallationStep[],
        currentStepId: null as string | null,
        errorMessage: '', // To store a global error message if needed
    }),

    actions: {
        reset() {
            this.steps = [];
            this.currentStepId = null;
            this.errorMessage = '';
            this.dialogOpen = false;
        },

        openDialog() {
            this.dialogOpen = true;
        },

        closeDialog() {
            // Prevent closing if running? Or allow with warning?
            // For now, simple close.
            this.dialogOpen = false;
        },

        setSteps(steps: InstallationStep[]) {
            this.steps = steps;
            if (steps.length > 0) {
                // Don't auto-start, wait for specific trigger if needed,
                // but usually we init with all pending.
            }
        },

        // New unified method to update step - handles the new StepUpdate event
        updateStep(updatedStep: InstallationStep) {
            const index = this.steps.findIndex(s => s.id === updatedStep.id);
            if (index !== -1) {
                // Update the step with new data
                this.steps[index] = { ...this.steps[index], ...updatedStep };

                // Track current running step
                if (updatedStep.status === 'running') {
                    this.currentStepId = updatedStep.id;
                }

                // Store error message if any
                if (updatedStep.error) {
                    this.errorMessage = updatedStep.error;
                }
            }
        },

        // Legacy methods - kept for backwards compatibility
        updateStepStatus(id: string, status: InstallationStatus) {
            const step = this.steps.find(s => s.id === id);
            if (step) {
                step.status = status;
                if (status === 'running') {
                    this.currentStepId = id;
                }
            }
        },

        setStepRunning(id: string) {
            this.updateStepStatus(id, 'running');
        },

        setStepSuccess(id: string) {
            this.updateStepStatus(id, 'success');
        },

        setStepError(id: string, message?: string) {
            this.updateStepStatus(id, 'error');
            const step = this.steps.find(s => s.id === id);
            if (step && message) {
                step.error = message;
            }
            if (message) {
                this.errorMessage = message;
            }
        },

        updateDownloadProgress(id: string, progress: number, speed: string, eta: string) {
            const step = this.steps.find(s => s.id === id);
            if (step && step.type === 'download') {
                step.progress = progress;
                step.downloadSpeed = speed;
                step.eta = eta;
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
