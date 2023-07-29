// Utilities

export function openUrlWithDefaultBrowser(url: string) {
    window.eel.open_url_in_default_browser(url)()
}
