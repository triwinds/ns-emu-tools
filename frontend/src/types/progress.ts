export type ProgressStatus = 'pending' | 'running' | 'success' | 'error' | 'cancelled';
export type ProgressStepType = 'normal' | 'download';

export interface ProgressStep {
    id: string;
    title: string;
    description?: string;
    status: ProgressStatus;
    type: ProgressStepType;
    // For download steps
    progress?: number; // 0-100
    downloadSpeed?: string;
    eta?: string;
    // Error message when status is 'error'
    error?: string;
    // Download source name (e.g., "直连", "Cloudflare CDN 负载均衡")
    downloadSource?: string;
}
