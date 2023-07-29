/**
 * plugins/vuetify.ts
 *
 * Framework documentation: https://vuetifyjs.com`
 */

// Styles
import '@mdi/font/css/materialdesignicons.css'
import 'vuetify/styles'

// Composables
import {createVuetify} from 'vuetify'
import { aliases, mdi } from 'vuetify/iconsets/mdi-svg'

// https://vuetifyjs.com/en/introduction/why-vuetify/#feature-guides
export default createVuetify({
  icons: {
    defaultSet: 'mdi',
    aliases,
    sets: {
      mdi,
    },
  },
  theme: {
    defaultTheme: 'light',
    themes: {
      dark: {
        colors: {
          primary: '#009688',
          secondary: '#89ddff',
          error: '#ff5370',
          warning: '#ffcb6b',
          info: '#89ddff',
          success: '#c3e88d',
          background: '#263238'
        }
      },
      light: {
        colors: {
          primary: '#3A66D1',
          secondary: '#2AA298',
          error: '#d25252',
          warning: '#E36209',
          info: '#2AA298',
          success: '#22863A',
          background: '#F7F8FA'
        }
      },
    },
  },
})
