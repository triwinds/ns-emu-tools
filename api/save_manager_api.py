from config import config
from api.common_response import *


@generic_api
def get_users_in_save():
    from module.save_manager import get_users_in_save
    return get_users_in_save()


@generic_api
def list_all_games_by_user_folder(folder: str):
    from module.save_manager import list_all_games_by_user_folder
    return list_all_games_by_user_folder(folder)


@generic_api
def ask_and_update_yuzu_save_backup_folder():
    from module.save_manager import ask_and_update_yuzu_save_backup_folder
    return ask_and_update_yuzu_save_backup_folder()


@generic_api
def backup_yuzu_save_folder(folder: str):
    from module.save_manager import backup_folder
    return backup_folder(folder)


@generic_api
def open_yuzu_save_backup_folder():
    from module.save_manager import open_yuzu_save_backup_folder
    return open_yuzu_save_backup_folder()


@generic_api
def list_all_yuzu_backups():
    from module.save_manager import list_all_yuzu_backups
    return list_all_yuzu_backups()


@generic_api
def restore_yuzu_save_from_backup(user_folder_name: str, backup_path: str):
    from module.save_manager import restore_yuzu_save_from_backup
    return restore_yuzu_save_from_backup(user_folder_name, backup_path)
