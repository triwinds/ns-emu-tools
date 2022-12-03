const {defineConfig} = require('@vue/cli-service')

module.exports = defineConfig({
    transpileDependencies: [
        'vuetify'
    ],
    outputDir: "../web",
    productionSourceMap: false,
    // chainWebpack: config => {
    //     // config.optimization.minimize(false)
    //     config.optimization.minimizer('terser').tap(options => {
    //         let option = options[0]
    //         option.exclude = /app/
    //         return options
    //     })
    // }
})
