import logging
import shutil
import time

from module.yuzu import get_yuzu_nand_path
from module.cheats.cheats import game_id_re
from module.msg_notifier import send_notify
from pathlib import Path
from storage import storage, dump_storage
from exception.common_exception import IgnoredException
from utils.package import compress_folder, uncompress, is_7zfile
from utils.common import is_path_in_use


logger = logging.getLogger(__name__)


def get_yuzu_save_path():
    # https://github.com/yuzu-emu/yuzu/blob/e264ab4ad0137224559ac0bacc68b905db65f5a8/src/core/file_sys/savedata_factory.cpp#L197
    return get_yuzu_nand_path().joinpath('user/save/0000000000000000')


def _get_all_user_ids():
    return [folder.name for folder in get_yuzu_save_path().glob('*') if len(folder.name) == 32]


def get_users_in_save():
    return [{'user_id': convert_to_uuid(uid), 'folder': uid} for uid in _get_all_user_ids()]


def convert_to_uuid(user_id: str):
    tmp = ''
    for i in range(16):
        tmp = user_id[i*2:i*2+2] + tmp
    return f'{tmp[:8]}-{tmp[8:12]}-{tmp[12:16]}-{tmp[16:20]}-{tmp[20:]}'.lower()


def list_all_games_by_user_folder(user_folder_name: str):
    user_save_folder = get_yuzu_save_path().joinpath(user_folder_name)
    res = []
    for folder in user_save_folder.glob('*'):
        if game_id_re.match(folder.name):
            res.append({
                'title_id': folder.name,
                'folder': str(folder.absolute())
            })
    return res


def backup_folder(folder_path: str):
    yuzu_save_backup_path = Path(storage.yuzu_save_backup_path)
    if not yuzu_save_backup_path.exists():
        yuzu_save_backup_path.mkdir(parents=True, exist_ok=True)
    folder_path = Path(folder_path)
    if is_path_in_use(folder_path):
        logger.info(f'{folder_path} is in use.')
        raise IgnoredException(f'{folder_path} 目录正在使用中，跳过备份。')
    backup_filename = f'yuzu_{folder_path.name}_{int(time.time())}.7z'
    backup_filepath = yuzu_save_backup_path.joinpath(backup_filename)
    logger.info(f'backup folder [{str(folder_path)}] to {str(backup_filepath)}')
    send_notify(f'正在备份文件夹 [{str(folder_path)}] 至 {str(backup_filepath)}')
    compress_folder(folder_path, backup_filepath)
    logger.info(f'{str(backup_filepath)} backup finished, size: {sizeof_fmt(backup_filepath.stat().st_size)}.')
    send_notify(f'{str(backup_filepath)} 备份完成, size: {sizeof_fmt(backup_filepath.stat().st_size)}')


def sizeof_fmt(num, suffix="B"):
    for unit in ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi"]:
        if abs(num) < 1024.0:
            return f"{num:3.1f}{unit}{suffix}"
        num /= 1024.0
    return f"{num:.1f}Yi{suffix}"


def ask_and_update_yuzu_save_backup_folder():
    from module.dialogs import ask_folder
    folder = ask_folder()
    logger.info(f'select yuzu_save_backup_folder: {folder}, current: {storage.yuzu_save_backup_path}')
    if not folder:
        send_notify('未选择文件夹, 取消变更.')
        return
    new_path = Path(folder).absolute()
    if Path(storage.yuzu_save_backup_path).absolute() == new_path:
        send_notify('文件夹未发生变动, 取消变更.')
        return
    storage.yuzu_save_backup_path = str(new_path.absolute())
    dump_storage()
    logger.info(f'new yuzu_save_backup_path: {storage.yuzu_save_backup_path}')
    send_notify(f'yuzu 存档备份文件夹更改为: {storage.yuzu_save_backup_path}')


def open_yuzu_save_backup_folder():
    import subprocess
    path = Path(storage.yuzu_save_backup_path)
    path.mkdir(parents=True, exist_ok=True)
    logger.info(f'open explorer on path {path}')
    subprocess.Popen(f'explorer "{str(path.absolute())}"')


def parse_backup_info(file: Path):
    res = {'filename': file.name, 'path': str(file.absolute())}
    if file.name.startswith('yuzu_') and file.name.endswith('.7z'):
        s = file.name[5:-3]
        title_id, bak_time = s.split('_')
        res['title_id'] = title_id
        res['bak_time'] = int(bak_time) * 1000
    return res


def list_all_yuzu_backups():
    path = Path(storage.yuzu_save_backup_path)
    res = []
    if not path.exists():
        return res
    for file in path.glob('yuzu_*.7z'):
        res.append(parse_backup_info(file))
    return sorted(res, key=lambda x: x['bak_time'], reverse=True)


def restore_yuzu_save_from_backup(user_folder_name: str, backup_path: str):
    backup_path = Path(backup_path)
    if not is_7zfile(backup_path):
        logger.info(f'{str(backup_path)} seems not a 7z file.')
        send_notify(f'{str(backup_path)} 看起来不是一个完整的 7z 文件，跳过还原.')
        return
    backup_info = parse_backup_info(backup_path)
    logger.info(f'backup_info: {backup_info}')
    user_save_path = get_yuzu_save_path().joinpath(user_folder_name)
    target_game_save_path = user_save_path.joinpath(backup_info['title_id'])
    if is_path_in_use(target_game_save_path):
        logger.info(f'{str(target_game_save_path)} is in use, skip restore.')
        send_notify(f'{str(target_game_save_path)} 目录正在使用中，跳过还原.')
        return
    logger.info(f'removing path: {str(target_game_save_path)}')
    send_notify(f'正在清空目录 {str(target_game_save_path)}')
    shutil.rmtree(target_game_save_path, ignore_errors=True)
    logger.info(f'uncompress to {str(target_game_save_path)}')
    send_notify(f'正在解压备份至 {str(user_save_path)}')
    uncompress(backup_path, user_save_path, False, '备份')
    logger.info(f'{str(backup_path)} restore done.')
    send_notify(f'{backup_path.name} 还原完成')


if __name__ == '__main__':
    # print(get_all_user_ids())
    # print(convert_to_uuid('97A1DAE861CD445AB9645267B3AB99BE'))
    print(get_users_in_save())
    # pprint(list_all_games_by_user_folder('97A1DAE861CD445AB9645267B3AB99BE'))
    # storage.yuzu_save_backup_path = 'R:/'
    # backup_folder('D:\\Yuzu\\user\\nand\\user\\save\\0000000000000000\\97A1DAE861CD445AB9645267B3AB99BE\\0100F3400332C000')
    # pprint(list_all_yuzu_backups())
    # restore_yuzu_save_from_backup('97A1DAE861CD445AB9645267B3AB99BE',
    #                               'D:\\yuzu_save_backup\\yuzu_0100F2C0115B6000_1685114415.7z')
