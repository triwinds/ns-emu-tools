/**
 * plugins/vuetify.ts
 *
 * Framework documentation: https://vuetifyjs.com`
 */

// Styles
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
  defaults: {
    VCardTitle: {
      style: 'padding-top: 16px; padding-bottom: 16px;'
    }
  },
  theme: {
    defaultTheme: 'dark',
    themes: {
      dark: {
        colors: {
          primary: '#009688',
          secondary: '#89ddff',
          accent: '#c792ea',
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
          accent: '#6F42C1',
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
