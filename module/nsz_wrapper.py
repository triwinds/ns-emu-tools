import os
import shutil
from logging import getLogger
from pathlib import Path

from exception.common_exception import IgnoredException

logger = getLogger(__name__)
_inited = False


def reload_key(key_path):
    global _inited
    try:
        if not _inited:
            _inited = True
            shutil.copy(key_path, './keys.txt')
            import nsz.nut.Print
            nsz.nut.Print.info = nsz.nut.Print.error = nsz.nut.Print.warning = nsz.nut.Print.debug = lambda *args: None
        from nsz.nut.Keys import load
        load(key_path)
        if not _inited:
            os.remove('./keys.txt')
    except:
        raise IgnoredException("Failed to load key")


def parse_nca_header(nca_path):
    if isinstance(nca_path, Path):
        nca_path = str(nca_path)
    from nsz.Fs.Nca import Nca
    nca = Nca()
    try:
        nca.open(nca_path)
        return nca.header
    finally:
        nca.close()


def read_firmware_version_from_nca(nca_path):
    if isinstance(nca_path, Path):
        nca_path = str(nca_path)
    from nsz.Fs.Nca import Nca
    nca = Nca()
    try:
        nca.open(nca_path)
        if not nca.sectionFilesystems:
            logger.info('No filesystem section found in nca.')
            return None
        data: bytearray = nca.sectionFilesystems[0].read()
        idx = data.index(b'NX\x00\x00\x00\x00') + 0x60
        # print(data[idx:])
        version = data[idx:idx + 0x10].replace(b'\x00', b'').decode()
        return version
    finally:
        nca.close()
