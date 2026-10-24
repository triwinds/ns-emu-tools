import { readonly, ref } from 'vue'
import { graphicsCommand } from './graphics'

export interface GraphicsGpu {
  hasNvidia: boolean
  srSupported: boolean
  nrSupported: boolean
  fgMaxMultiplier: number
  adapters: readonly { name: string; vendorId: number }[]
}
const capabilities = ref<GraphicsGpu | null>(null)
const error = ref('')
let pending: Promise<void> | undefined
export function useGraphicsGpu() {
  async function load(force = false) {
    if (pending) return pending
    if (capabilities.value && !force) return
    error.value = ''
    pending = graphicsCommand<GraphicsGpu>('get_graphics_gpu_capabilities')
      .then(value => { capabilities.value = value })
      .catch(e => { capabilities.value = null; error.value = String(e) })
      .finally(() => { pending = undefined })
    return pending
  }
  return { capabilities: readonly(capabilities), error: readonly(error), load }
}
