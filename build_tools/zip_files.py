from utils.package import compress_folder
from pathlib import Path


DIST_FOLDER = Path(__file__).parent.parent / 'dist'


if __name__ == '__main__':
    compress_folder(DIST_FOLDER.joinpath('NsEmuTools'), DIST_FOLDER.joinpath('NsEmuTools.7z'))
