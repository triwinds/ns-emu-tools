class FailToCopyFiles(Exception):
    raw_exception: Exception
    msg: str

    def __init__(self, raw_exception: Exception, msg):
        self.raw_exception = raw_exception
        self.msg = msg
        super().__init__(msg)
