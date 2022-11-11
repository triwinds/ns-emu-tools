import Vue from 'vue';
import Vuetify from 'vuetify/lib/framework';
import 'vuetify/dist/vuetify.min.css'
// import 'typeface-roboto'

Vue.use(Vuetify);

export default new Vuetify({
    icons: {
        iconfont: 'mdiSvg',
    },
    theme: {
        dark: true,
        // color palette: https://material-theme.com/docs/reference/color-palette/
        themes: {
            dark: {
                primary: '#009688',
                secondary: '#89ddff',
                accent: '#c792ea',
                error: '#ff5370',
                warning: '#ffcb6b',
                info: '#89ddff',
                success: '#c3e88d',
                background: '#263238'
            },
            light: {
                primary: '#3A66D1',
                secondary: '#2AA298',
                accent: '#6F42C1',
                error: '#d25252',
                warning: '#E36209',
                info: '#2AA298',
                success: '#22863A',
                background: '#F7F8FA'
            },
        },
    },
})
