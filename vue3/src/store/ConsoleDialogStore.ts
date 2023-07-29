// Utilities
import {defineStore} from 'pinia'

export const useConsoleDialogStore = defineStore('consoleDialog', {
    state: () => ({
        dialogFlag: false,
        persistentConsoleDialog: false,
        consoleMessages: [] as string[],
    }),
    actions: {
        cleanAndShowConsoleDialog() {
            this.dialogFlag = true
            this.consoleMessages = []
        },
        showConsoleDialog() {
            this.dialogFlag = true
        },
        appendConsoleMessage(message: string) {
            if (!message) {
                return
            }
            const splits = message.split('\n')
            for (const value of splits) {
                if (value.length < 1) {
                    continue
                }
                if (value && value.startsWith('下载速度: ') && this.consoleMessages.length > 0
                    && this.consoleMessages[this.consoleMessages.length - 1].startsWith('下载速度: ')) {
                    this.consoleMessages[this.consoleMessages.length - 1] = value
                } else {
                    this.consoleMessages.push(value)
                }
            }
        }
    }
})
