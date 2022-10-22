from config import yuzu_config
from module.yuzu import install_yuzu, install_firmware_to_yuzu, install_key_to_yuzu


if __name__ == '__main__':
    print(f'Yuzu path: {yuzu_config.yuzu_path}')
    print(f'Yuzu version: {yuzu_config.yuzu_version}')
    print(f'Yuzu firmware: {yuzu_config.yuzu_firmware}')
    install_yuzu()
    install_firmware_to_yuzu()
    install_key_to_yuzu()
    print(f'Yuzu version: {yuzu_config.yuzu_version}')
    print(f'Yuzu firmware: {yuzu_config.yuzu_firmware}')
