

class VersionNotFoundException(Exception):
    msg: str = ''
    target_version: str = ''
    branch: str = ''
    emu_type: str = ''

    def __init__(self, target_version, branch, emu_type):
        self.target_version = target_version
        self.branch = branch
        self.emu_type = emu_type
        self.msg = f'Fail to get release info of version [{target_version}] on branch [{branch}]'
        super().__init__(self.msg)


class Md5NotMatchException(Exception):
    def __init__(self):
        super().__init__('MD5 not match')


class IgnoredException(RuntimeError):
    def __init__(self, msg=''):
        super().__init__(msg)
