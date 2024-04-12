from pathlib import Path
import logging

import py7zr

from exception.common_exception import IgnoredException
from module.msg_notifier import send_notify
import os


logger = logging.getLogger(__name__)


def uncompress(filepath: Path, target_path, delete_on_error=True,
               exception_msg='当前下载的文件看起来不太正常，请重新下载试试'):
    if isinstance(target_path, str):
        target_path = Path(target_path)
    try:
        if filepath.name.lower().endswith(".zip"):
            import zipfile
            with zipfile.ZipFile(filepath, 'r') as zf:
                zf.extractall(str(target_path.absolute()))
        elif filepath.name.lower().endswith(".7z"):
            import py7zr
            with py7zr.SevenZipFile(filepath) as zf:
                zf.extractall(str(target_path.absolute()))
        elif filepath.name.lower().endswith(".tar.xz"):
            import tarfile
            with tarfile.open(filepath, 'r') as tf:
                tf.extractall(str(target_path.absolute()))
    except Exception as e:
        logger.error(f'Fail to uncompress file: {filepath}', exc_info=True)
        if delete_on_error:
            send_notify(f'文件解压失败，正在删除异常的文件 [{filepath}]')
            os.remove(filepath)
        raise IgnoredException(exception_msg)


def compress_folder(folder_path: Path, save_path):
    import py7zr
    if isinstance(save_path, str):
        save_path = Path(save_path)
    directory = str(folder_path.absolute())
    rootdir = os.path.basename(directory)
    try:
        logger.info(f'compress {folder_path} to {save_path}')
        zf: py7zr.SevenZipFile
        with py7zr.SevenZipFile(save_path, 'w') as zf:
            for dirpath, dirnames, filenames in os.walk(directory):
                for filename in filenames:
                    # Write the file named filename to the archive,
                    # giving it the archive name 'arcname'.
                    filepath = os.path.join(dirpath, filename)
                    parentpath = os.path.relpath(filepath, directory)
                    arcname = os.path.join(rootdir, parentpath)
                    zf.write(filepath, arcname)
    except Exception as e:
        logger.error(f'Fail to compress file {folder_path} to {save_path}', exc_info=True)
        raise IgnoredException(f'备份失败, {str(e)}')


def is_7zfile(filepath: Path):
    return py7zr.is_7zfile(filepath)
