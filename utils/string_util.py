import chardet


def auto_decode(input_bytes: bytes):
    det_res = chardet.detect(input_bytes)
    if det_res['encoding']:
        return input_bytes.decode(det_res['encoding'])
    return input_bytes.decode()
