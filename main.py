import argparse
import logging
import gevent.monkey

gevent.monkey.patch_ssl()
gevent.monkey.patch_socket()

import sys
from config import config, dump_config
from utils.webview2 import can_use_webview

logger = logging.getLogger(__name__)


def start_ui(mode=None):
    import ui
    ui.main(mode=mode)
    return 0


def start_webview_ui():
    import ui_webview
    ui_webview.main()
    return 0


def try_start_webview():
    try:
        return start_webview_ui()
    except Exception as e:
        logger.error('Error occur in start_webview_ui', e)
        return fallback_to_browser()


def fallback_to_browser():
    config.setting.ui.mode = 'browser'
    dump_config()
    return start_ui(None)


def create_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-m",
        "--mode",
        choices=['webview', 'browser', 'chrome', 'edge', 'user default'],
        help="指定 ui 启动方式",
    )
    parser.add_argument(
        "--switch-mode",
        choices=['auto', 'webview', 'browser', 'chrome', 'edge', 'user default'],
        help="切换 ui 启动方式",
    )
    parser.add_argument(
        "--no-sentry",
        action='store_true',
        help="禁用 sentry",
    )
    return parser


def main():
    parser = create_parser()
    args = parser.parse_args()
    logger.info(f'args: {args}')

    if args.switch_mode is not None:
        logger.info(f'switch mode: {args.switch_mode}')
        config.setting.ui.mode = args.switch_mode
        dump_config()
        return 0

    from module.external.bat_scripts import create_scripts
    create_scripts()

    if not args.no_sentry:
        from module.sentry import init_sentry
        init_sentry()

    ui_mode = args.mode or config.setting.ui.mode
    logger.info(f'ui mode: {ui_mode}')
    if ui_mode is None or ui_mode == 'auto':
        ui_mode = 'webview' if can_use_webview() else 'browser'
    if ui_mode == 'browser':
        return start_ui(None)
    elif ui_mode == 'webview':
        return try_start_webview()
    else:
        return start_ui(ui_mode)


if __name__ == '__main__':
    sys.exit(main())
