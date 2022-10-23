import eel
from config import yuzu_config
from repository.yuzu import get_all_yuzu_release_infos
from module.common import get_firmware_infos

eel.init("web")


@eel.expose
def get_yuzu_config():
    return yuzu_config.to_dict()


@eel.expose
def get_yuzu_release_infos():
    return get_all_yuzu_release_infos()


@eel.expose
def get_available_firmware_infos():
    return get_firmware_infos()


@eel.expose
def install_yuzu(version):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.yuzu import install_yuzu
    return {'msg': install_yuzu(version)}


@eel.expose
def install_firmware(version):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.yuzu import install_firmware_to_yuzu
    return {'msg': install_firmware_to_yuzu(version)}


@eel.expose
def get_available_keys_info():
    from module.common import get_keys_info
    return get_keys_info()


@eel.expose
def install_keys(name):
    if not name or name == '':
        return {'msg': f'无效的 key {name}'}
    from module.yuzu import install_key_to_yuzu
    return {'msg': install_key_to_yuzu(name)}


def can_use_chrome():
    """ Identify if Chrome is available for Eel to use """
    import os
    from eel import chrome
    chrome_instance_path = chrome.find_path()
    return chrome_instance_path is not None and os.path.exists(chrome_instance_path)


def main():
    from module.msg_notifier import update_notifier
    update_notifier('eel')
    if can_use_chrome():
        eel.start("index.html", port=0)
    else:
        eel.start("index.html", port=0, mode='user default')


if __name__ == '__main__':
    main()
