import type { ProgressStep } from '../types/progress';

const MIN_RUNNING_DISPLAY_MS = 300;

// Keep backend state immediate while presenting fast sequential phases in order.
export class ProgressDisplay {
    private steps: ProgressStep[];
    private queue: ProgressStep[] = [];
    private startedAt = new Map<string, number>();
    private titles = new Map<string, string>();
    private timer?: ReturnType<typeof setTimeout>;
    private waiters: Array<() => void> = [];

    constructor(steps: ProgressStep[], private publish: (steps: ProgressStep[], busy: boolean) => void) {
        this.steps = steps.map(step => ({ ...step }));
        for (const step of steps) {
            if (step.status === 'running') this.startedAt.set(step.id, Date.now());
        }
        this.publish(this.steps, false);
    }

    update(step: ProgressStep) {
        const last = this.queue[this.queue.length - 1];
        if (last?.id === step.id && last.status === 'running' && step.status === 'running') {
            this.queue[this.queue.length - 1] = { ...last, ...step };
        } else {
            this.queue.push({ ...step });
        }
        this.flush(step.status === 'error' || step.status === 'cancelled');
    }

    setCompletionTitle(title: string) {
        const last = this.steps[this.steps.length - 1];
        if (!last) return;
        this.titles.set(last.id, title);
        this.steps = this.steps.map(step => step.id === last.id ? { ...step, title } : step);
        this.publish(this.steps, this.queue.length > 0);
    }

    whenIdle(): Promise<void> {
        return this.queue.length ? new Promise(resolve => this.waiters.push(resolve)) : Promise.resolve();
    }

    dispose() {
        clearTimeout(this.timer);
        this.queue = [];
        this.resolveWaiters();
    }

    private resolveWaiters() {
        for (const resolve of this.waiters.splice(0)) resolve();
    }

    private flush(immediate = false) {
        clearTimeout(this.timer);
        while (this.queue.length) {
            const update = this.queue[0];
            const previous = this.steps.find(step => step.id === update.id);
            const started = this.startedAt.get(update.id);
            if (!immediate && previous?.status === 'running' && update.status === 'success' && started !== undefined) {
                const remaining = MIN_RUNNING_DISPLAY_MS - (Date.now() - started);
                if (remaining > 0) {
                    this.timer = setTimeout(() => this.flush(), remaining);
                    break;
                }
            }
            this.queue.shift();
            if (!previous) continue;
            if (update.status === 'running') {
                if (previous.status !== 'running') this.startedAt.set(update.id, Date.now());
            } else {
                this.startedAt.delete(update.id);
            }
            const title = this.titles.get(update.id) ?? update.title;
            this.steps = this.steps.map(step => step.id === update.id ? { ...step, ...update, title } : step);
        }
        this.publish(this.steps, this.queue.length > 0);
        if (!this.queue.length) this.resolveWaiters();
    }
}
