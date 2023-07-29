// Composables
import mitt from 'mitt'

const emitter = mitt()

export function useEmitter() {
  return emitter
}
