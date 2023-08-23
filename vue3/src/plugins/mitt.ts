// Composables
import mitt from 'mitt'

export const emitter = mitt()

export function useEmitter() {
  return emitter
}
