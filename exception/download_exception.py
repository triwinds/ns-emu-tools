
class DownloadInterrupted(Exception):
    def __init__(self):
        super().__init__('Download has been interrupted')


class DownloadPaused(Exception):
    def __init__(self):
        super().__init__('Download has been paused')


class DownloadNotCompleted(Exception):
    status: str
    name: str

    def __init__(self, name, status):
        self.name = name
        self.status = status
        super().__init__(f'Download task [{name}] is not completed, status: {status}')
