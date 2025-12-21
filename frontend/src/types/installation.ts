export type InstallationStatus = 'pending' | 'running' | 'success' | 'error' | 'cancelled';
export type InstallationStepType = 'normal' | 'download';

export interface InstallationStep {
    id: string;
    title: string;
    description?: string;
    status: InstallationStatus;
    type: InstallationStepType;
    // For download steps
    progress?: number; // 0-100
    downloadSpeed?: string;
    eta?: string;
    // Error message when status is 'error'
    error?: string;
}
