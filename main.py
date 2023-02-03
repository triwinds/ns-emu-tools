from config import config, dump_config
import argparse
from utils.webview2 import ensure_runtime_components, can_use_webview, show_msgbox


def start_ui(mode=None):
    import ui
    ui.main(mode=mode)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-m",
        "--mode",
        choices=['webview', 'browser', 'chrome', 'edge', 'user default'],
        help="指定 ui 启动方式",
    )
    args = parser.parse_args()
    ui_mode = args.mode or config.setting.ui.mode
    ui_mode = None if ui_mode == 'browser' else ui_mode
    if ui_mode == 'webview':
        if can_use_webview():
            import ui_webview
            ui_webview.main()
        else:
            ret = show_msgbox('部分组件缺失',
                              '由于部分组件缺失，无法以 webview 方式启动，是否安装相关组件？\n'
                              '(选否将以浏览器方式启动，老版本的浏览器可能存在兼容性问题，不推荐)', 4)
            if ret == 7:
                config.setting.ui.mode = 'browser'
                dump_config()
                start_ui(ui_mode)
            ensure_runtime_components()
    else:
        start_ui(ui_mode)
