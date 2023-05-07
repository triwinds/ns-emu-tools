from pathlib import Path
import logging
from exception.common_exception import IgnoredException
from module.msg_notifier import send_notify
import os


logger = logging.getLogger(__name__)


def uncompress(filepath: Path, target_path, delete_on_error=True):
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
    except Exception as e:
        logger.error(f'Fail to uncompress file: {filepath}', exc_info=True)
        if delete_on_error:
            send_notify(f'文件解压失败，正在删除异常的文件 [{filepath}]')
            os.remove(filepath)
        raise IgnoredException('当前下载的文件看起来不太正常，请重新下载试试')
