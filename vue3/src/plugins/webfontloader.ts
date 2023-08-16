/**
 * plugins/webfontloader.ts
 *
 * webfontloader documentation: https://github.com/typekit/webfontloader
 */
import '@fontsource-variable/jetbrains-mono'

export async function loadFonts () {
  const webFontLoader = await import(/* webpackChunkName: "webfontloader" */'webfontloader')

  webFontLoader.load({
    // google: {
    //   families: ['JetBrains Mono'],
    // },
    custom: {
      families: ['JetBrains Mono Variable']
    }
  })
}
