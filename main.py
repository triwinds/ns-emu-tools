from config import config, dump_config
import argparse
from utils.webview2 import ensure_runtime_components, can_use_webview, show_msgbox
import logging


logger = logging.getLogger(__name__)


def start_ui(mode=None):
    import ui
    ui.main(mode=mode)
    return 0


def start_webview_ui():
    import ui_webview
    ui_webview.main()
    return 0


def fallback_to_browser():
    config.setting.ui.mode = 'browser'
    dump_config()
    return start_ui(None)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-m",
        "--mode",
        choices=['webview', 'browser', 'chrome', 'edge', 'user default'],
        help="指定 ui 启动方式",
    )
    args = parser.parse_args()
    ui_mode = args.mode or config.setting.ui.mode
    if ui_mode == 'browser':
        ui_mode = None
    if ui_mode != 'webview':
        return start_ui(ui_mode)
    if can_use_webview():
        try:
            return start_webview_ui()
        except Exception as e:
            logger.error('Error occur in start_webview_ui', e)
            fallback_to_browser()
    ret = show_msgbox('部分组件缺失',
                      '由于部分组件缺失，无法以 webview 方式启动，是否安装相关组件？\n'
                      '(选否将以浏览器方式启动，老版本的浏览器可能存在兼容性问题，不推荐)', 4)
    if ret == 7:
        return fallback_to_browser()
    ensure_runtime_components()
    return 0


if __name__ == '__main__':
    exit(main())
