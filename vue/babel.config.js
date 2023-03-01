module.exports = {
    presets: [
        '@vue/cli-plugin-babel/preset',
        [
            '@babel/preset-env',
            {
                useBuiltIns: 'entry', // or "usage"
                corejs: 3,
            },
        ]
    ]
}
