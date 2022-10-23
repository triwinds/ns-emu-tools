
def dummy_notifier(msg):
    pass


def eel_notifier(msg):
    import eel
    eel.updateTopBarMsg(msg)


notifier = dummy_notifier


def update_notifier(mode):
    global notifier
    if mode == 'eel':
        notifier = eel_notifier
    else:
        notifier = dummy_notifier


def send_notify(msg):
    notifier(msg)
