from typing import List
from config import config
from api.common_response import *

import eel


@eel.expose
def scan_all_cheats_folder():
    from module.cheats import scan_all_cheats_folder
    from module.yuzu import get_yuzu_load_path
    try:
        return success_response(scan_all_cheats_folder(get_yuzu_load_path()))
    except Exception as e:
        return exception_response(e)


@eel.expose
def list_all_cheat_files_from_folder(folder_path: str):
    from module.cheats import list_all_cheat_files_from_folder
    try:
        return success_response(list_all_cheat_files_from_folder(folder_path))
    except Exception as e:
        return exception_response(e)


@eel.expose
def load_cheat_chunk_info(cheat_file_path: str):
    from module.cheats import load_cheat_chunk_info
    try:
        return success_response(load_cheat_chunk_info(cheat_file_path))
    except Exception as e:
        return exception_response(e)


@eel.expose
def update_current_cheats(enable_titles: List[str], cheat_file_path: str):
    from module.cheats import update_current_cheats
    try:
        return success_response(update_current_cheats(enable_titles, cheat_file_path))
    except Exception as e:
        return exception_response(e)


@eel.expose
def open_cheat_mod_folder(folder_path: str):
    from module.cheats import open_cheat_mod_folder
    try:
        return success_response(open_cheat_mod_folder(folder_path))
    except Exception as e:
        return exception_response(e)


@eel.expose
def get_game_data():
    from module.cheats import get_game_data
    try:
        return success_response(get_game_data())
    except Exception as e:
        return exception_response(e)
