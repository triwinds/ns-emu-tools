import Vue from 'vue';
import Vuetify from 'vuetify/lib/framework';

Vue.use(Vuetify);

export default new Vuetify({
    theme: {
        dark: true,
        themes: {
            dark: {
                // color palette: https://material-theme.com/docs/reference/color-palette/
                primary: '#009688',
                secondary: '#89ddff',
                accent: '#c792ea',
                error: '#ff5370',
                warning: '#ffcb6b',
                info: '#89ddff',
                success: '#c3e88d',
                background: '#263238'
            },
        },
    },
})
