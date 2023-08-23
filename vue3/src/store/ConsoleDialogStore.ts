// Utilities
import {defineStore} from 'pinia'

export const useConsoleDialogStore = defineStore('consoleDialog', {
    state: () => ({
        dialogFlag: false,
        persistentConsoleDialog: false,
        consoleMessages: [] as string[],
        newLine: true
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
                if (value && value.startsWith('^')) {
                    if (this.newLine) {
                        this.consoleMessages.push(value)
                        this.newLine = false
                    } else {
                        this.consoleMessages[this.consoleMessages.length - 1] = value.substring(1)
                    }
                } else {
                    this.consoleMessages.push(value)
                    if (!this.newLine) {
                        this.newLine = true
                    }
                }
            }
        },
        cleanMessages() {
            this.consoleMessages = []
        }
    }
})
