from config import yuzu_config
from module.yuzu import install_yuzu, install_firmware_to_yuzu, install_key_to_yuzu
import argparse


def start_ui():
    import ui
    ui.main()


def run_in_cli():
    print(f'Yuzu path: {yuzu_config.yuzu_path}')
    print(f'Yuzu version: {yuzu_config.yuzu_version}')
    print(f'Yuzu firmware: {yuzu_config.yuzu_firmware}')
    install_yuzu()
    install_firmware_to_yuzu()
    install_key_to_yuzu()
    print(f'Yuzu version: {yuzu_config.yuzu_version}')
    print(f'Yuzu firmware: {yuzu_config.yuzu_firmware}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-nu",
        "--no-ui",
        action="store_true",
        help="do not open a browser to show the application and simply print out where it's being hosted from. "
             "When using this option, you must manually stop the application using Ctrl+C",
    )
    args = parser.parse_args()
    if args.no_ui:
        run_in_cli()
    else:
        start_ui()
