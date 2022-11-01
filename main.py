from config import config
import argparse


def start_ui():
    import ui
    ui.main()


# def run_in_cli():
#     from module.yuzu import install_yuzu, install_firmware_to_yuzu, install_key_to_yuzu
#     print(f'Yuzu path: {config.yuzu.yuzu_path}')
#     print(f'Yuzu version: {config.yuzu.yuzu_version}')
#     print(f'Yuzu firmware: {config.yuzu.yuzu_firmware}')
#     install_yuzu()
#     install_firmware_to_yuzu()
#     install_key_to_yuzu()
#     print(f'Yuzu version: {config.yuzu.yuzu_version}')
#     print(f'Yuzu firmware: {config.yuzu.yuzu_firmware}')


if __name__ == '__main__':
    start_ui()
    # parser = argparse.ArgumentParser()
    # parser.add_argument(
    #     "-nu",
    #     "--no-ui",
    #     action="store_true",
    #     help="Run in cli mode.",
    # )
    # args = parser.parse_args()
    # if args.no_ui:
    #     run_in_cli()
    # else:
    #     start_ui()
